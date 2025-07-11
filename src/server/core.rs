use super::client_stub::*;
use super::super::transport::Transport;
use super::Task;
use crate::directory::*;
use crate::protocol::*;
use crate::stopper::BasicStopper;
use crate::topic::*;
use crate::err::*;
use std::collections::HashMap;
use tokio::time::Duration;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

pub(crate) type ClientId = u32;

struct PendingResponse{
    request_id: MsgId,
    responder_id: ClientId,
    requester_id: ClientId
}

pub(crate) struct Core {
    task_sender: UnboundedSender<Task>,
    task_receiver: UnboundedReceiver<Task>,
    protocol_server_senders: HashMap<u32, UnboundedSender<Msg<ProtocolServer>>>,
    directory: Directory,
    next_client_id: u32,
    next_msg_id: u32,
    pending_responses: HashMap<u32, PendingResponse>
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
            pending_responses: HashMap::new()
        };

        core.directory
            .subscribe(0, &parse_topic("bus/kill").unwrap());

        core
    }

    pub fn get_task_sender(&mut self) -> UnboundedSender<Task> {
        self.task_sender.clone()
    }

    pub fn spawn(mut self) -> BusResult<BasicStopper> {
        let (stop_sender, stop_receiver) = tokio::sync::oneshot::channel();
        let join_handle = tokio::spawn(async move {
            #[cfg(debug_assertions)]
            println!("[B] Bus server started");

            let result = self.run(stop_receiver).await;

            #[cfg(debug_assertions)]
            println!("[B] Bus server stopped: {result:?}");

            result
        });
        Ok(BasicStopper::new(stop_sender, join_handle))
    }

    async fn run(&mut self, mut stop_receiver: tokio::sync::oneshot::Receiver<()>) -> BusResult<()> {
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
                let rsp = RspMsg {
                    req_id,
                    status: RspMsgStatus::Timeout,
                    payload: vec![].into(),
                };

                self.respond(0, rsp)?;

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
            println!("[B] Client #{client_id} connected");

            match client.serve().await {
                Ok(_) | Err(BusError::ChannelClosed) => {}
                Err(e) => println!("[B] Client #{client_id}: Error: {e:?}")
            }

            println!("[B] Client #{client_id} disconnected");
        });

        self.protocol_server_senders
            .insert(client_id, protocol_server_sender);
        Ok(())
    }

    fn deregister_client(&mut self, id: u32) -> BusResult<()> {
        self.protocol_server_senders.remove(&id);
        self.directory.drop_client(id);
        self.pending_responses.retain(|_, v| v.requester_id != id && v.responder_id != id);
        Ok(())
    }

    fn process_msg(&mut self, client_id: u32, msg: Msg<ProtocolClient>) -> BusResult<bool> {
        #[cfg(debug_assertions)]
        {
            let log_msg = crate::debug::client_msg_to_string(&msg);
            println!("[B] <-- [{}] {}", client_id, &log_msg);
        }

        let stop = matches!(msg.content, ProtocolClient::Stop);

        let result = match msg.content {
            ProtocolClient::Pub(params) => self
                .publish(params)
                .map(Some),
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
        let topic = parse_topic(&params.topic)?;
        self.directory.claim(client_id, &topic)?;

        Ok(())
    }

    fn unserve(&mut self, client_id: u32, params: UnsrvMsg) -> BusResult<()> {
        let topic = parse_topic(&params.topic)?;
        self.directory.unclaim(client_id, &topic)?;

        Ok(())
    }

    fn request(&mut self, client_id: u32, req_id: u32, params: ReqMsg) -> BusResult<()> {
        let topic = parse_topic(&params.topic)?;
        let server_id_opt = self.directory.get_owner(&topic);

        match server_id_opt {
            Some(server_id) => {
                let req = ProtocolServer::Req(ReqMsg {
                    topic: params.topic,
                    payload: params.payload,
                });

                let msg_id = self.deliver(server_id, req)?;

                let pending_response = PendingResponse{
                    request_id: req_id,
                    requester_id: client_id,
                    responder_id: server_id
                };

                self.pending_responses.insert(msg_id, pending_response);

                let task_sender_clone = self.task_sender.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_secs(REQUEST_TIMEOUT_S)).await;
                    let task = Task::RequestTimeout(msg_id);
                    let _ = task_sender_clone.send(task);
                });

                Ok(())
            }
            None => Err(BusError::ServiceNotFound(params.topic.clone())),
        }
    }

    fn respond(&mut self, client_id: ClientId, params: RspMsg) -> BusResult<()> {
        let pending_response_option = self.pending_responses.get(&params.req_id);
        if let Some(&PendingResponse{request_id, requester_id, responder_id, .. }) = pending_response_option {

            // The responder is trying to respond to a request that wasn't sent to them
            if client_id != 0 && responder_id != client_id {
                return Err(BusError::InvalidRequestId)
            };

            self.pending_responses.remove(&params.req_id);

            let rsp = ProtocolServer::Rsp(RspMsg {
                req_id: request_id,
                status: params.status,
                payload: params.payload,
            });

            self.deliver(requester_id, rsp)?;
        } else {
            return Err(BusError::InvalidRequestId)
        }

        Ok(())
    }

    fn subscribe(&mut self, client_id: u32, params: SubMsg) -> BusResult<()> {
        let topic = parse_topic(&params.topic)?;
        self.directory.subscribe(client_id, &topic);

        Ok(())
    }

    fn unsubscribe(&mut self, client_id: u32, params: UnsubMsg) -> BusResult<()> {
        let topic = parse_topic(&params.topic)?;
        self.directory.unsubscribe(client_id, &topic)?;

        Ok(())
    }

    fn publish(&mut self, params: PubMsg) -> BusResult<usize> {
        let topic = parse_topic(&params.topic)?;
        let subscriber_ids = self.directory.get_subscribers(&topic);

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

        #[cfg(debug_assertions)]
        {
            let log_msg = crate::debug::server_msg_to_string(&msg);
            println!("[B] --> [{}] {}", client_id, &log_msg);
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

                        #[cfg(debug_assertions)]
                        println!("Client #{} message delivery failed: {}", client_id, &e);

                        Err(BusError::DeliveryFailed(client_id, e.to_string()))
                    }
                }
            }
            None => Err(BusError::UnknownClient(client_id))
        }
    }

    fn msg(&mut self, content: ProtocolServer) -> Msg<ProtocolServer> {
        let id = self.next_msg_id;
        self.next_msg_id += 1;

        Msg {
            id,
            content
        }
    }
}