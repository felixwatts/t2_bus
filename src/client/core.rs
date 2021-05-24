use crate::protocol::*;
use crate::topic::*;
use crate::{directory::Directory, transport::Transport};

use crate::err::*;
use std::collections::HashMap;

use futures::SinkExt;
use futures::StreamExt;
use tokio::task::JoinHandle;

pub(crate) struct TaskPub {
    pub(crate) msg: MsgPub,
    pub(crate) callback_ack: tokio::sync::oneshot::Sender<MsgAck>,
}
pub(crate) struct TaskSub {
    pub(crate) msg: MsgSub,
    pub(crate) callback_ack: tokio::sync::oneshot::Sender<MsgAck>,
    pub(crate) callback_pub: tokio::sync::mpsc::UnboundedSender<MsgPub>,
}

pub(crate) struct TaskUnsub {
    pub(crate) msg: MsgUnsub,
}

pub(crate) struct TaskUnsrv {
    pub(crate) msg: MsgUnsrv,
}

pub(crate) struct TaskReq {
    pub(crate) msg: MsgReq,
    pub(crate) callback_ack: tokio::sync::oneshot::Sender<MsgAck>,
    pub(crate) callback_rsp: tokio::sync::oneshot::Sender<MsgRsp>,
}

pub(crate) struct TaskRsp {
    pub(crate) msg: MsgRsp,
    pub(crate) callback_ack: tokio::sync::oneshot::Sender<MsgAck>,
}

pub(crate) struct TaskSrv {
    pub(crate) msg: MsgSrv,
    pub(crate) callback_ack: tokio::sync::oneshot::Sender<MsgAck>,
    pub(crate) callback_req: tokio::sync::mpsc::UnboundedSender<(MsgId, MsgReq)>,
}

pub(crate) struct TaskStopBus {
    pub(crate) callback_ack: tokio::sync::oneshot::Sender<MsgAck>,
}

pub(crate) enum Task {
    Pub(TaskPub),
    Sub(TaskSub),
    Req(TaskReq),
    Rsp(TaskRsp),
    Srv(TaskSrv),
    Unsub(TaskUnsub),
    Unsrv(TaskUnsrv),
    Stop,
    StopBus(TaskStopBus),
}

pub(crate) struct ClientCore<TTransport>
where
    TTransport: Transport<ProtocolClient, ProtocolServer>,
{
    next_msg_id: MsgId,
    transport: TTransport,
    callbacks_ack: HashMap<MsgId, tokio::sync::oneshot::Sender<MsgAck>>,
    callbacks_rsp: HashMap<MsgId, tokio::sync::oneshot::Sender<MsgRsp>>,
    callbacks_pub: HashMap<MsgId, tokio::sync::mpsc::UnboundedSender<MsgPub>>,
    callbacks_req: HashMap<MsgId, tokio::sync::mpsc::UnboundedSender<(MsgId, MsgReq)>>,
    subscriptions: Directory,
    task_receiver: tokio::sync::mpsc::UnboundedReceiver<Task>,
}

impl<TTransport> ClientCore<TTransport>
where
    TTransport: Transport<ProtocolClient, ProtocolServer>,
{
    pub fn start(
        transport: TTransport,
        task_receiver: tokio::sync::mpsc::UnboundedReceiver<Task>,
    ) -> BusResult<JoinHandle<BusResult<()>>> {
        let mut core = ClientCore {
            transport,
            subscriptions: Directory::new(),
            task_receiver: task_receiver,
            next_msg_id: 0,
            callbacks_ack: HashMap::new(),
            callbacks_rsp: HashMap::new(),
            callbacks_pub: HashMap::new(),
            callbacks_req: HashMap::new(),
        };

        let handle = tokio::spawn(async move { core.main_loop().await });

        Ok(handle)
    }

    async fn main_loop(&mut self) -> BusResult<()> {
        loop {
            tokio::select! {
                // message from server
                msg = self.transport.next() => {
                    self.after_receive(msg.ok_or(BusError::ChannelClosed)??).await?;
                },
                // message from client
                task_option = self.task_receiver.recv() => {
                    match task_option {
                        Some(task) => {
                            if let Task::Stop = task {
                                return Ok(())
                            }

                            self.send(task).await?;
                        },
                        None => {
                            return Ok(())
                        }
                    }
                }
            }
        }
    }

    async fn send(&mut self, task: Task) -> BusResult<()> {
        let msg = self.before_send(task)?;

        #[cfg(debug_assertions)]
        {
            let log_msg = crate::debug::client_msg_to_string(&msg);
            println!("[B] <-- [?] {}", &log_msg);
        }

        self.transport.send(msg).await?;
        Ok(())
    }

    fn before_send(&mut self, task: Task) -> BusResult<Msg<ProtocolClient>> {
        self.next_msg_id += 1;
        let id = self.next_msg_id;

        let msg = match task {
            Task::Pub(params) => {
                self.callbacks_ack.insert(id, params.callback_ack);
                Msg {
                    id: id,
                    content: ProtocolClient::Pub(params.msg),
                }
            }
            Task::Sub(params) => {
                self.callbacks_ack.insert(id, params.callback_ack);
                self.callbacks_pub.insert(id, params.callback_pub);
                self.subscriptions
                    .subscribe(id, &parse_topic(&params.msg.topic)?);
                Msg {
                    id: id,
                    content: ProtocolClient::Sub(params.msg),
                }
            }
            Task::Req(params) => {
                self.callbacks_ack.insert(id, params.callback_ack);
                self.callbacks_rsp.insert(id, params.callback_rsp);
                Msg {
                    id: id,
                    content: ProtocolClient::Req(params.msg),
                }
            }
            Task::Rsp(params) => {
                self.callbacks_ack.insert(id, params.callback_ack);
                Msg {
                    id: id,
                    content: ProtocolClient::Rsp(params.msg),
                }
            }
            Task::Unsub(params) => Msg {
                id: id,
                content: ProtocolClient::Unsub(params.msg),
            },
            Task::Unsrv(params) => {
                let topic = parse_topic(&params.msg.topic)?;
                let callback_id = self.subscriptions.get_owner(&topic).unwrap();
                self.subscriptions.unclaim(callback_id, &topic)?;
                Msg {
                    id: id,
                    content: ProtocolClient::Unsrv(params.msg),
                }
            }
            Task::Srv(params) => {
                self.callbacks_ack.insert(id, params.callback_ack);
                self.callbacks_req.insert(id, params.callback_req);
                self.subscriptions
                    .claim(id, &parse_topic(&params.msg.topic)?)?;
                Msg {
                    id: id,
                    content: ProtocolClient::Srv(params.msg),
                }
            }
            Task::Stop => panic!(),
            Task::StopBus(params) => {
                self.callbacks_ack.insert(id, params.callback_ack);
                Msg {
                    id: id,
                    content: ProtocolClient::Stop,
                }
            }
        };

        Ok(msg)
    }

    async fn after_receive(&mut self, msg: Msg<ProtocolServer>) -> BusResult<()> {
        #[cfg(debug_assertions)]
        {
            let log_msg = crate::debug::server_msg_to_string(&msg);
            println!("[B] --> [?] {}", &log_msg);
        }

        match msg.content {
            ProtocolServer::Ack(payload) => {
                let _ = self
                    .callbacks_ack
                    .remove(&payload.msg_id)
                    .map(|c| c.send(payload));
            }
            ProtocolServer::Rsp(payload) => {
                let callback = self.callbacks_rsp.remove(&payload.req_id).unwrap();
                let _ = callback.send(payload); // ignore result, client isn't obliged to wait for RSP and may have dropped the channel
            }
            ProtocolServer::Pub(payload) => {
                let callback_ids = self
                    .subscriptions
                    .get_subscribers(&parse_topic(&payload.topic)?);
                for callback_id in callback_ids {
                    // call the callback
                    let callback_result = self
                        .callbacks_pub
                        .get_mut(&callback_id)
                        .unwrap()
                        .send(payload.clone());

                    if let Err(_) = callback_result {
                        // The subscriber dropped the callback channel

                        // remove local subscriber
                        self.callbacks_pub.remove(&callback_id);
                        let topics = self.subscriptions.drop_client(callback_id);

                        // unsubscribe from server for any topics that now have no subscribers
                        for topic in topics {
                            let unsubscribe_task = Task::Unsub(TaskUnsub {
                                msg: MsgUnsub { topic: topic },
                            });
                            self.send(unsubscribe_task).await?;

                            // NOTE: we cannot wait for an ack because we are already handling
                            // an incoming message so are blocking the incoming message processing task
                            // expect_ack(&mut callback_ack_receiver).await?;
                        }
                    }
                }
            }
            ProtocolServer::Req(payload) => {
                let callback_id_option =
                    self.subscriptions.get_owner(&parse_topic(&payload.topic)?);
                match callback_id_option {
                    Some(callback_id) => {
                        let callback = self.callbacks_req.get_mut(&callback_id).unwrap();
                        callback.send((msg.id, payload))?;
                    }
                    None => {
                        // TODO respond so requester always gets a response?
                    }
                }
            }
        }

        Ok(())
    }
}
