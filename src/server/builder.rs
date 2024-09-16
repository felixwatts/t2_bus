use std::{net::SocketAddr, path::PathBuf};
use futures::{future::join_all, stream::{FuturesOrdered, FuturesUnordered}, StreamExt};
use tokio::{net::{ToSocketAddrs}, sync::mpsc::UnboundedSender};
use crate::{err::BusResult, stopper::{BasicStopper, MultiStopper}, transport::{memory::{MemoryConnector, MemoryListener}, tcp::TcpListener, unix::UnixListener}};
use crate::server::Task;
use super::core::Core;

pub struct ServerBuilder{
    listeners: Vec<ListenerEnum>
}

impl Default for ServerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ServerBuilder{
    pub fn new() -> Self{
        ServerBuilder{
            listeners: vec![]
        }
    }
    
    pub fn serve_unix_socket(mut self, addr: PathBuf) -> Self {
        self.listeners.push(ListenerEnum::Unix(addr));
        self
    }

    pub fn serve_tcp(mut self, addr: SocketAddr) -> Self {
        self.listeners.push(ListenerEnum::Tcp(addr));
        self
    }

    pub fn serve_memory(mut self) -> Self {
        if !self.listeners.iter().any(|l| matches!(l, ListenerEnum::Memory)) {
            self.listeners.push(ListenerEnum::Memory);
        }
        self
    }

    pub async fn build(mut self) -> BusResult<(MultiStopper, Option<MemoryConnector>)> {
        let mut core = Core::new();
        let listen_results = join_all(self
            .listeners
            .drain(..)
            .map(|l| l.listen(core.get_task_sender())))
            .await
            .into_iter()
            .collect::<BusResult<Vec<(BasicStopper, Option<MemoryConnector>)>>>()?;
        let core_stopper = core.spawn()?;
        let (mut stoppers, memory_connector) = listen_results
            .into_iter()
            .fold((vec![], None), |(mut stoppers, memory_connector), next_result| { 
                stoppers.push(next_result.0);
                let memory_connector = memory_connector.or(next_result.1);
                (stoppers, memory_connector)
            });
        stoppers.push(core_stopper);
        let stopper = MultiStopper::new(stoppers);
        Ok((stopper, memory_connector))
    }
}

enum ListenerEnum{
    Memory,
    Tcp(SocketAddr),
    // Tls(TlsListener),
    Unix(PathBuf)
}

impl ListenerEnum{
    async fn listen(self, register_channel: UnboundedSender<Task>) -> BusResult<(BasicStopper, Option<MemoryConnector>)>{
        match self{
            ListenerEnum::Memory => {
                let (listener, connector) = MemoryListener::new();
                let stopper = crate::server::listen::listen(listener, register_channel)?;
                Ok((stopper, Some(connector)))
            },
            ListenerEnum::Unix(addr) => {
                let listener = UnixListener::new(&addr)?;
                let stopper = crate::server::listen::listen(listener, register_channel)?;
                Ok((stopper, None))
            },
            ListenerEnum::Tcp(addr) => {
                let listener = TcpListener::new(addr).await?;
                let stopper = crate::server::listen::listen(listener, register_channel)?;
                Ok((stopper, None))
            },
        }
    }
}