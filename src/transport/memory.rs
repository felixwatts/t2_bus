use futures::{Sink, Stream};
use tokio::{sync::mpsc::{UnboundedReceiver, UnboundedSender}};

use crate::{
    err::*, protocol::{Msg, ProtocolClient, ProtocolServer}, server::{listen::Listener}
};

use super::Transport;

pub struct MemoryTransport<TSend, TRecv> {
    sender: UnboundedSender<BusResult<Msg<TSend>>>,
    receiver: UnboundedReceiver<BusResult<Msg<TRecv>>>,
}

pub(crate) struct MemoryListener {    
    accept_receiver: UnboundedReceiver<MemoryTransport<ProtocolServer, ProtocolClient>>,
}

impl Listener for MemoryListener{
    async fn accept(&mut self) -> BusResult<impl Transport<ProtocolServer, ProtocolClient>> {
        let client = self.accept_receiver.recv().await;
        match client{
            Some(client) => Ok(client),
            None => Err(BusError::ChannelClosed)
        }
    }
}

pub struct MemoryConnector{
    accept_sender: UnboundedSender<MemoryTransport<ProtocolServer, ProtocolClient>>,
}

impl MemoryConnector{
    pub fn connect(&self) -> BusResult<MemoryTransport<ProtocolClient, ProtocolServer>> {
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

        self.accept_sender.send(server_side)?;

        Ok(client_side)
    }
}

impl MemoryListener {
    pub(crate) fn new() -> (Self, MemoryConnector){
        let (accept_sender, accept_receiver) = tokio::sync::mpsc::unbounded_channel();
        let listener = Self { accept_receiver };
        let connector = MemoryConnector{ accept_sender };
        (listener, connector)
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
