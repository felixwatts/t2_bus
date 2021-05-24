use futures::{Sink, Stream};
use tokio::{sync::mpsc::{UnboundedReceiver, UnboundedSender}, task::JoinHandle};

use crate::{
    protocol::{Msg, ProtocolClient, ProtocolServer},
    server::Task,
    err::*,
};

use super::Transport;

pub struct MemoryTransport<TSend, TRecv> {
    sender: UnboundedSender<BusResult<Msg<TSend>>>,
    receiver: UnboundedReceiver<BusResult<Msg<TRecv>>>,
}

pub struct MemoryTransportListener {
    register_channel: UnboundedSender<Task>,
}

pub fn listen_and_serve() -> BusResult<(MemoryTransportListener, tokio::sync::oneshot::Sender<()>, JoinHandle<BusResult<()>>)> {
    let mut core = crate::server::Core::new();
    let memory_listener = MemoryTransportListener { register_channel: core.get_task_sender() };
    let (core_stop_sender, core_join_handle) = core.spawn()?;
    Ok((memory_listener, core_stop_sender, core_join_handle))
}

impl MemoryTransportListener {
    pub fn connect(&mut self) -> BusResult<MemoryTransport<ProtocolClient, ProtocolServer>> {
        let (client_sender, server_receiver) = tokio::sync::mpsc::unbounded_channel();
        let (server_sender, client_receiver) = tokio::sync::mpsc::unbounded_channel();

        let client_side = MemoryTransport {
            sender: client_sender,
            receiver: client_receiver,
        };

        let server_side = MemoryTransport {
            sender: server_sender,
            receiver: server_receiver,
        };

        self.register_channel
            .send(Task::Register(Box::new(server_side)))?;

        Ok(client_side)
    }
}

impl<TSend, TRecv> Stream for MemoryTransport<TSend, TRecv> {
    type Item = BusResult<Msg<TRecv>>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.get_mut().receiver.poll_recv(cx)
    }
}

impl<TSend, TRecv> Sink<Msg<TSend>> for MemoryTransport<TSend, TRecv> {
    type Error = BusError;

    fn poll_ready(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn start_send(self: std::pin::Pin<&mut Self>, item: Msg<TSend>) -> Result<(), Self::Error> {
        self.sender.send(Ok(item))?;
        Ok(())
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn poll_close(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }
}

impl<TSend, TRecv> Transport<TSend, TRecv> for MemoryTransport<TSend, TRecv>
where
    TSend: 'static + serde::Serialize + Send,
    TRecv: 'static + serde::de::DeserializeOwned + Send,
{
}
