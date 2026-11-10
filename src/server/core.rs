use super::super::transport::Transport;
use super::client_stub::*;
use super::Task;
use crate::directory::*;
use crate::err::*;
use crate::protocol::*;
use crate::stopper::BasicStopper;
use log::{debug, info, log_enabled, trace, warn, Level};
use std::collections::HashMap;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::task::AbortHandle;
use tokio::time::Duration;

pub(crate) type ClientId = u32;

struct PendingResponse {
    request_id: MsgId,
    responder_id: ClientId,
    requester_id: ClientId,
    _timeout: TimeoutTimer,
}

/// Cancels the request's timeout timer when dropped, so that answered requests
/// do not leave a sleeping task behind for the rest of the timeout period.
struct TimeoutTimer(AbortHandle);

impl Drop for TimeoutTimer {
    fn drop(&mut self) {
        self.0.abort();
    }
}

pub(crate) struct Core {
    task_sender: UnboundedSender<Task>,
    task_receiver: UnboundedReceiver<Task>,
    protocol_server_senders: HashMap<u32, UnboundedSender<Msg<ProtocolServer>>>,
    directory: Directory,
    next_client_id: u32,
    next_msg_id: u32,
    pending_responses: HashMap<u32, PendingResponse>,
}

impl Core {
    pub fn new() -> Core {
        let (task_sender, task_receiver) = unbounded_channel();

        let mut core = Core {
            task_sender,
            task_receiver,
            next_client_id: 1,
            protocol_server_senders: HashMap::new(),
            directory: Directory::new(),
            next_msg_id: 0,
            pending_responses: HashMap::new(),
        };

        core.directory.subscribe(0, &"bus:kill".parse().unwrap());

        core
    }

    pub fn get_task_sender(&mut self) -> UnboundedSender<Task> {
        self.task_sender.clone()
    }

    pub fn spawn(mut self) -> BusResult<BasicStopper> {
        let (stop_sender, stop_receiver) = tokio::sync::oneshot::channel();
        let join_handle = tokio::spawn(async move {
            debug!("[B] Bus server started");

            let result = self.run(stop_receiver).await;

            debug!("[B] Bus server stopped: {result:?}");

            result
        });
        Ok(BasicStopper::new(stop_sender, join_handle))
    }

    async fn run(
        &mut self,
        mut stop_receiver: tokio::sync::oneshot::Receiver<()>,
    ) -> BusResult<()> {
        loop {
            tokio::select! {
                _ = &mut stop_receiver => return Ok(()),
                task = self.task_receiver.recv() => {
                    let stop = match task {
                        None => true,
                        Some(msg) => self.process_task(msg)?,
                    };
                    if stop {
                        return Ok(());
                    }
                }
            }
        }
    }

    fn process_task(&mut self, task: Task) -> BusResult<bool> {
        let stop = match task {
            Task::Register(transport) => {
                self.register_client(transport)?;
                false
            }
            Task::Deregister(client_id) => {
                self.deregister_client(client_id)?;
                false
            }
            Task::Message(client_id, msg) => self.process_msg(client_id, msg)?,
            Task::RequestTimeout(req_id) => {
                // The timer may fire just before the request is answered or its
                // requester disconnects, in which case it is no longer pending.
                // That is not an error, and an error here would stop the server.
                if self.pending_responses.contains_key(&req_id) {
                    let rsp = RspMsg {
                        req_id,
                        status: RspMsgStatus::Timeout,
                        payload: vec![].into(),
                    };

                    if let Err(e) = self.respond(0, rsp) {
                        warn!("[B] Failed to deliver timeout for request {req_id}: {e:?}");
                    }
                }

                false
            }
        };

        Ok(stop)
    }

    fn register_client(
        &mut self,
        transport: Box<dyn Transport<ProtocolServer, ProtocolClient>>,
    ) -> BusResult<()> {
        let client_id = self.next_client_id;
        self.next_client_id += 1;

        let (client, protocol_server_sender) =
            ClientStub::new(client_id, transport, self.task_sender.clone())?;

        tokio::spawn(async move {
            info!("[B] Client #{client_id} connected");

            match client.serve().await {
                Ok(_) | Err(BusError::ChannelClosed) => {}
                Err(e) => warn!("Client #{client_id} error: {e:?}"),
            }

            info!("[B] Client #{client_id} disconnected");
        });

        self.protocol_server_senders
            .insert(client_id, protocol_server_sender);
        Ok(())
    }

    fn deregister_client(&mut self, id: u32) -> BusResult<()> {
        self.protocol_server_senders.remove(&id);
        self.directory.drop_client(id);
        self.pending_responses
            .retain(|_, v| v.requester_id != id && v.responder_id != id);
        Ok(())
    }

    fn process_msg(&mut self, client_id: u32, msg: Msg<ProtocolClient>) -> BusResult<bool> {
        if log_enabled!(Level::Trace) {
            trace!(
                "[B] <-- [{client_id}] {}",
                crate::debug::client_msg_to_string(&msg)
            );
        }

        let stop = matches!(msg.content, ProtocolClient::Stop);

        let result = match msg.content {
            ProtocolClient::Pub(params) => self.publish(params).map(Some),
            _ => {
                let result = match msg.content {
                    ProtocolClient::Sub(params) => self.subscribe(client_id, params),
                    ProtocolClient::Unsub(params) => self.unsubscribe(client_id, params),
                    ProtocolClient::Srv(params) => self.serve(client_id, params),
                    ProtocolClient::Unsrv(params) => self.unserve(client_id, params),
                    ProtocolClient::Req(params) => self.request(client_id, msg.id, params),
                    ProtocolClient::Rsp(params) => self.respond(client_id, params),
                    ProtocolClient::Stop => Ok(()),
                    ProtocolClient::KeepAlive => Ok(()),
                    ProtocolClient::Pub(_) => panic!(),
                };
                result.map(|_| None)
            }
        };

        let ack = match result {
            Ok(num_recipients) => AckMsg {
                msg_id: msg.id,
                err: None,
                num_recipients,
            },
            Err(err) => AckMsg {
                msg_id: msg.id,
                err: Some(err),
                num_recipients: None,
            },
        };

        let ack = ProtocolServer::Ack(ack);

        // failure to deliver an Ack can be ignored
        let _ = self.deliver(client_id, ack);

        Ok(stop)
    }

    fn serve(&mut self, client_id: u32, params: SrvMsg) -> BusResult<()> {
        self.directory.claim(client_id, &params.topic)?;

        Ok(())
    }

    fn unserve(&mut self, client_id: u32, params: UnsrvMsg) -> BusResult<()> {
        self.directory.unclaim(client_id, &params.topic)?;

        Ok(())
    }

    fn request(&mut self, client_id: u32, req_id: u32, params: ReqMsg) -> BusResult<()> {
        let server_id_opt = self.directory.get_owner(&params.topic);

        match server_id_opt {
            Some(server_id) => {
                let req = ProtocolServer::Req(ReqMsg {
                    topic: params.topic,
                    payload: params.payload,
                });

                let msg_id = self.deliver(server_id, req)?;

                let task_sender_clone = self.task_sender.clone();
                let timer = tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_secs(REQUEST_TIMEOUT_S)).await;
                    let task = Task::RequestTimeout(msg_id);
                    let _ = task_sender_clone.send(task);
                });

                let pending_response = PendingResponse {
                    request_id: req_id,
                    requester_id: client_id,
                    responder_id: server_id,
                    _timeout: TimeoutTimer(timer.abort_handle()),
                };

                self.pending_responses.insert(msg_id, pending_response);

                Ok(())
            }
            None => Err(BusError::ServiceNotFound(params.topic)),
        }
    }

    fn respond(&mut self, client_id: ClientId, params: RspMsg) -> BusResult<()> {
        let pending_response_option = self.pending_responses.get(&params.req_id);
        if let Some(&PendingResponse {
            request_id,
            requester_id,
            responder_id,
            ..
        }) = pending_response_option
        {
            // The responder is trying to respond to a request that wasn't sent to them
            if client_id != 0 && responder_id != client_id {
                return Err(BusError::InvalidRequestId);
            };

            self.pending_responses.remove(&params.req_id);

            let rsp = ProtocolServer::Rsp(RspMsg {
                req_id: request_id,
                status: params.status,
                payload: params.payload,
            });

            self.deliver(requester_id, rsp)?;
        } else {
            return Err(BusError::InvalidRequestId);
        }

        Ok(())
    }

    fn subscribe(&mut self, client_id: u32, params: SubMsg) -> BusResult<()> {
        self.directory.subscribe(client_id, &params.topic);

        Ok(())
    }

    fn unsubscribe(&mut self, client_id: u32, params: UnsubMsg) -> BusResult<()> {
        self.directory.unsubscribe(client_id, &params.topic)?;

        Ok(())
    }

    fn publish(&mut self, params: PubMsg) -> BusResult<usize> {
        let subscriber_ids = self.directory.get_subscribers(&params.topic);

        let payload = ProtocolServer::Pub(PubMsg {
            topic: params.topic,
            payload: params.payload,
        });

        for client_id in &subscriber_ids {
            if *client_id == 0 {
                panic!("Received kill message"); // TODO make nicer
            }

            self.deliver(*client_id, payload.clone())?;
        }

        Ok(subscriber_ids.len())
    }

    fn deliver(&mut self, client_id: ClientId, payload: ProtocolServer) -> BusResult<MsgId> {
        let msg = self.msg(payload);

        if log_enabled!(Level::Trace) {
            trace!(
                "[B] --> [{client_id}] {}",
                crate::debug::server_msg_to_string(&msg)
            );
        }

        let client_opt = self.protocol_server_senders.get_mut(&client_id);
        match client_opt {
            Some(client) => {
                let msg_id = msg.id;
                let send_result = client.send(msg);
                match send_result {
                    Ok(_) => Ok(msg_id),
                    Err(e) => {
                        // client has disconnected, cleanup
                        self.deregister_client(client_id)?;

                        warn!("[B] Client #{client_id} message delivery failed: {e}");

                        Err(BusError::DeliveryFailed(client_id, e.to_string()))
                    }
                }
            }
            None => Err(BusError::UnknownClient(client_id)),
        }
    }

    fn msg(&mut self, content: ProtocolServer) -> Msg<ProtocolServer> {
        let id = self.next_msg_id;
        self.next_msg_id += 1;

        Msg { id, content }
    }
}
