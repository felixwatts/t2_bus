use std::path::PathBuf;

use crate::{client::Client, err::BusResult, transport::memory::MemoryConnector};

#[derive(Clone)]
pub enum Connector{
    Memory(MemoryConnector),
    Unix(PathBuf),
    Tcp(String)
}

impl Connector{
    pub async fn connect(&self) -> BusResult<Client>{
        match self{
            Connector::Memory(c) => Client::new(c.connect()?),
            Connector::Unix(addr) => crate::transport::unix::connect(addr).await,
            Connector::Tcp(addr) => crate::transport::tcp::connect(addr).await
        }
    }

    pub fn new_tcp(addr: String) -> Self{
        Self::Tcp(addr)
    }

    pub fn new_unix(addr: PathBuf) -> Self {
        Self::Unix(addr)
    }
}