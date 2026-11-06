pub mod connector;
mod core;
pub mod subscription;

use self::core::*;
use self::subscription::*;
use super::transport::Transport;
use crate::err::*;
use crate::protocol::*;
use crate::topic::{Path, Topic};

use std::time::Duration;

use tokio::{
    sync::mpsc::unbounded_channel, sync::mpsc::UnboundedSender, task::JoinHandle, time::timeout,
};

const ACK_TIMEOUT_S: u64 = 5;

/// A client of the bus. Provides a client-side API for all bus features including publish, subscribe, request and respond.
// #[derive(Clone)]
pub struct Client {
    task_sender: tokio::sync::mpsc::UnboundedSender<Task>,
    core_join_handle: JoinHandle<BusResult<()>>,
}

impl Client {
    pub(crate) fn new(
        transport: impl Transport<ProtocolClient, ProtocolServer>,
    ) -> BusResult<Client> {
        let (task_sender, task_receiver) = tokio::sync::mpsc::unbounded_channel();

        let core_join_handle = ClientCore::start(transport, task_receiver)?;

        let client = Client {
            task_sender,
            core_join_handle,
        };

        Ok(client)
    }

    /// Send a stop message to the bus itself. The bus will terminate.
    pub async fn stop_bus(&mut self) -> BusResult<()> {
        let (callback_ack_sender, mut callback_ack_receiver) = tokio::sync::oneshot::channel();

        let task = Task::StopBus(TaskStopBus {
            callback_ack: callback_ack_sender,
        });

        self.task_sender.send(task)?;
        expect_ack(&mut callback_ack_receiver).await?;

        Ok(())
    }

    /// Cleanly shutdown this client
    pub async fn stop(self) -> BusResult<()> {
        let _ = self.task_sender.send(Task::Stop); // ignore error if channel closed
        self.core_join_handle.await??;
        Ok(())
    }

    /// Start serving the protocol `TProtocol` at the given path.
    ///
    /// Requests on this protocol and path will be routed to you via the returned `RequestSubscription`.
    /// Drop the `RequestSubscription` to stop serving.
    /// Errors if the specified protocol and path is already being served by someone else.
    pub async fn serve<TProtocol>(&self, path: Path) -> BusResult<RequestSubscription<TProtocol>>
    where
        TProtocol: RequestProtocol,
    {
        let topic = Topic::new(TProtocol::prefix(), path)?;

        let (callback_ack_sender, mut callback_ack_receiver) = tokio::sync::oneshot::channel();
        let (callback_req_sender, mut callback_req_receiver) =
            tokio::sync::mpsc::unbounded_channel();

        let (typed_request_sender, typed_request_receiver) = unbounded_channel();
        let subscription = RequestSubscription::new(
            topic.clone(),
            typed_request_receiver,
            self.task_sender.clone(),
        );

        let task = Task::Srv(TaskSrv {
            msg: SrvMsg { topic },
            callback_ack: callback_ack_sender,
            callback_req: callback_req_sender,
        });

        self.task_sender.send(task)?;
        expect_ack(&mut callback_ack_receiver).await?;

        tokio::spawn(async move {
            while let Some((req_id, msg_req)) = callback_req_receiver.recv().await {
                let data: Vec<u8> = msg_req.payload.into();
                let req: TProtocol = crate::transport::cbor_codec::deser(&data[..]).unwrap();

                let (_, path) = msg_req.topic.into_parts();
                let send_result = typed_request_sender.send((path, req_id, req));

                if send_result.is_err() {
                    return;
                }
            }
        });

        Ok(subscription)
    }

    /// Make a request to the protocol `TProtocol` at the given path. Returns the response or an error if
    /// no one is serving on the protocol and path.
    pub async fn request<TProtocol>(&self, path: Path, req: &TProtocol) -> BusResult<TProtocol::Rsp>
    where
        TProtocol: RequestProtocol,
    {
        let topic = Topic::new(TProtocol::prefix(), path)?;

        let payload = crate::transport::cbor_codec::ser(req)?;

        let (callback_ack_sender, mut callback_ack_receiver) = tokio::sync::oneshot::channel();
        let (callback_rsp_sender, callback_rsp_receiver) = tokio::sync::oneshot::channel();

        let task = Task::Req(TaskReq {
            msg: ReqMsg {
                topic,
                payload: payload.into(),
            },
            callback_ack: callback_ack_sender,
            callback_rsp: callback_rsp_sender,
        });

        self.task_sender.send(task)?;
        expect_ack(&mut callback_ack_receiver).await?;

        let rsp = callback_rsp_receiver.await?;

        match rsp.status {
            RspMsgStatus::Ok => {
                let data: Vec<u8> = rsp.payload.into();
                let rsp = crate::transport::cbor_codec::deser(&data[..])?;
                Ok(rsp)
            }
            RspMsgStatus::Timeout => Err(BusError::RequestFailedTimeout),
        }
    }

    /// Similar to `request` but takes a full `Topic` (protocol and path) and both the request and response
    /// payloads are `Vec<u8>`s that should be encoded according to the protocol.
    pub async fn request_bytes(&mut self, topic: Topic, payload: Vec<u8>) -> BusResult<Vec<u8>> {
        let (callback_ack_sender, mut callback_ack_receiver) = tokio::sync::oneshot::channel();
        let (callback_rsp_sender, callback_rsp_receiver) = tokio::sync::oneshot::channel();

        let task = Task::Req(TaskReq {
            msg: ReqMsg {
                topic,
                payload: payload.into(),
            },
            callback_ack: callback_ack_sender,
            callback_rsp: callback_rsp_sender,
        });

        self.task_sender.send(task)?;
        expect_ack(&mut callback_ack_receiver).await?;

        let rsp = callback_rsp_receiver.await?;

        Ok(rsp.payload.into())
    }

    /// Respond to a received request. The `request_id` parameter should correspond to that of a received request.
    /// The given response will be routed to the original requester.
    pub async fn respond<TProtocol>(&self, request_id: MsgId, rsp: &TProtocol::Rsp) -> BusResult<()>
    where
        TProtocol: RequestProtocol,
    {
        let payload = crate::transport::cbor_codec::ser(rsp)?;
        self._respond(request_id, RspMsgStatus::Ok, payload).await
    }

    /// Subscribe to the protocol `TProtocol` at paths matching the given path. Values published on the protocol and
    /// matching paths will be routed to you, together with the path they were published at, via the returned
    /// `Subscription`. Drop the `Subscription` to unsubscribe.
    pub async fn subscribe<TProtocol>(
        &self,
        path: Path,
    ) -> BusResult<Subscription<(Path, TProtocol)>>
    where
        TProtocol: PublishProtocol,
    {
        let (callback_pub_sender, callback_pub_receiver) = unbounded_channel::<(Path, TProtocol)>();
        let subscription_into = self
            .subscribe_into::<TProtocol>(path, callback_pub_sender)
            .await?;
        let subscription = Subscription::new(callback_pub_receiver, subscription_into);
        Ok(subscription)
    }

    /// Similar to `subscribe` except published values are sent to the given `UnboundedSender`. Useful
    /// if you want to combine several subscriptions into a single channel.
    /// Returns a `SubscriptionInto` which can be dropped to unsubscribe.
    pub async fn subscribe_into<TProtocol>(
        &self,
        path: Path,
        callback_sender: UnboundedSender<(Path, TProtocol)>,
    ) -> BusResult<SubscriptionInto>
    where
        TProtocol: PublishProtocol,
    {
        let topic = Topic::new(TProtocol::prefix(), path)?;

        let (callback_ack_sender, mut callback_ack_receiver) = tokio::sync::oneshot::channel();
        let (callback_pub_sender, mut callback_pub_receiver) = unbounded_channel::<PubMsg>();

        tokio::spawn(async move {
            while let Some(msg_pub) = callback_pub_receiver.recv().await {
                let data: Vec<u8> = msg_pub.payload.into();
                let msg: TProtocol = crate::transport::cbor_codec::deser(&data)
                    .map_err(|e| {
                        BusError::MalformedMessage(TProtocol::prefix().to_string(), e.to_string())
                    })
                    .unwrap();

                let (_, path) = msg_pub.topic.into_parts();
                let result = callback_sender.send((path, msg));
                if result.is_err() {
                    return;
                }
            }
        });

        let task = Task::Sub(TaskSub {
            msg: SubMsg {
                topic: topic.clone(),
            },
            callback_ack: callback_ack_sender,
            callback_pub: callback_pub_sender,
        });

        self.task_sender.send(task)?;

        expect_ack(&mut callback_ack_receiver).await?;

        let subscription = SubscriptionInto::new(topic, self.task_sender.clone());

        Ok(subscription)
    }

    /// Similar to `subscribe` but takes a full `Topic` (protocol and path) and the received
    /// payloads are `Vec<u8>`s that should be encoded according to the protocol. Useful when the protocol
    /// is not known at compile time. The protocol may be the wildcard `*` to match all protocols.
    pub async fn subscribe_bytes(&self, topic: Topic) -> BusResult<Subscription<PubMsg>> {
        let (sender, receiver) = unbounded_channel::<PubMsg>();
        let sub_into = self.subscribe_bytes_into(topic, sender).await?;
        let sub = Subscription::new(receiver, sub_into);
        Ok(sub)
    }

    /// Similar to `subscribe_bytes` except published values are sent to the given `UnboundedSender`. Useful
    /// if you want to combine several subscriptions into a single channel.
    /// Returns a `SubscriptionInto` which can be dropped to unsubscribe.
    pub async fn subscribe_bytes_into(
        &self,
        topic: Topic,
        sender: UnboundedSender<PubMsg>,
    ) -> BusResult<SubscriptionInto> {
        let (callback_ack_sender, mut callback_ack_receiver) = tokio::sync::oneshot::channel();

        let task = Task::Sub(TaskSub {
            msg: SubMsg {
                topic: topic.clone(),
            },
            callback_ack: callback_ack_sender,
            callback_pub: sender,
        });

        self.task_sender.send(task)?;

        expect_ack(&mut callback_ack_receiver).await?;

        let subscription = SubscriptionInto::new(topic, self.task_sender.clone());

        Ok(subscription)
    }

    /// Publish the given message at the given path. Returns the number of receiving clients.
    pub async fn publish<TProtocol>(&self, path: Path, msg: &TProtocol) -> BusResult<usize>
    where
        TProtocol: PublishProtocol,
    {
        let topic = Topic::new(TProtocol::prefix(), path)?;

        let payload = crate::transport::cbor_codec::ser(msg)?;

        let num_recipients = self.publish_bytes(topic, payload).await?;
        Ok(num_recipients)
    }

    /// Similar to `publish` but takes a full `Topic` (protocol and path) and the
    /// payload is a `Vec<u8>` that should be encoded according to the protocol. Useful when the protocol
    /// is not known at compile time.
    pub async fn publish_bytes(&self, topic: Topic, payload: Vec<u8>) -> BusResult<usize> {
        let (callback_ack_sender, mut callback_ack_receiver) = tokio::sync::oneshot::channel();

        let task = Task::Pub(TaskPub {
            msg: PubMsg {
                topic,
                payload: payload.into(),
            },
            callback_ack: callback_ack_sender,
        });

        self.task_sender.send(task)?;
        let num_recipients = expect_ack(&mut callback_ack_receiver).await?.unwrap();

        Ok(num_recipients)
    }

    async fn _respond(
        &self,
        req_id: MsgId,
        status: RspMsgStatus,
        payload: Vec<u8>,
    ) -> BusResult<()> {
        let (callback_ack_sender, mut callback_ack_receiver) = tokio::sync::oneshot::channel();

        let task = Task::Rsp(TaskRsp {
            msg: RspMsg {
                req_id,
                status,
                payload: payload.into(),
            },
            callback_ack: callback_ack_sender,
        });

        self.task_sender.send(task)?;
        expect_ack(&mut callback_ack_receiver).await?;

        Ok(())
    }
}

async fn expect_ack(
    receiver: &mut tokio::sync::oneshot::Receiver<AckMsg>,
) -> BusResult<Option<usize>> {
    match timeout(Duration::from_secs(ACK_TIMEOUT_S), receiver).await {
        Ok(ack) => match ack {
            Ok(ack) => match ack.err {
                Some(err) => Err(BusError::RequestFailed(err.to_string())),
                None => Ok(ack.num_recipients),
            },
            _ => Err(BusError::RequestFailedChannelClosed),
        },
        _ => Err(BusError::RequestFailedTimeout),
    }
}
