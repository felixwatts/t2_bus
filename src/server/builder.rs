use std::path::PathBuf;
use tokio::{net::ToSocketAddrs, sync::mpsc::UnboundedSender};
use crate::{err::BusResult, stopper::{BasicStopper, MultiStopper}, transport::{memory::{MemoryConnector, MemoryListener}, tcp::TcpListener, unix::UnixListener}};
use crate::server::Task;
use super::core::Core;

pub struct ServerBuilder{
    listeners: Vec<ListenerEnum>,
    memory_connector: Option<MemoryConnector>
}

impl Default for ServerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ServerBuilder{
    pub fn new() -> Self{
        ServerBuilder{
            listeners: vec![],
            memory_connector: None
        }
    }
    
    pub fn serve_unix_socket(mut self, addr: &PathBuf) -> BusResult<Self> {
        self.listeners.push(ListenerEnum::Unix(UnixListener::new(addr)?));
        Ok(self)
    }

    pub async fn serve_tcp(mut self, addr: impl ToSocketAddrs) -> BusResult<Self> {
        self.listeners.push(ListenerEnum::Tcp(TcpListener::new(addr).await?));
        Ok(self)
    }

    pub async fn serve_memory(mut self) -> BusResult<Self> {
        if self.listeners.iter().any(|l| matches!(l, ListenerEnum::Memory(_))) {
            return Ok(self);
        }
        let (listener, connector) = MemoryListener::new();
        self.memory_connector = Some(connector);
        self.listeners.push(ListenerEnum::Memory(listener));
        Ok(self)
    }

    pub fn build(mut self) -> BusResult<(MultiStopper, Option<MemoryConnector>)> {
        let mut core = Core::new();
        let mut stoppers = self
            .listeners
            .drain(..)
            .map(|l| l.listen(core.get_task_sender()))
            .collect::<BusResult<Vec<BasicStopper>>>()?;
        let core_stopper = core.spawn()?;
        stoppers.push(core_stopper);
        let stopper = MultiStopper::new(stoppers);
        Ok((stopper, self.memory_connector))
    }
}

enum ListenerEnum{
    Memory(MemoryListener),
    Tcp(TcpListener),
    // Tls(TlsListener),
    Unix(UnixListener)
}

impl ListenerEnum{
    fn listen(self, register_channel: UnboundedSender<Task>) -> BusResult<BasicStopper>{
        match self{
            ListenerEnum::Memory(l) => crate::server::listen::listen(l, register_channel),
            ListenerEnum::Unix(l) => crate::server::listen::listen(l, register_channel),
            ListenerEnum::Tcp(l) => crate::server::listen::listen(l, register_channel),
        }
    }
}