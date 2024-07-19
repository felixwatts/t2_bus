mod client_stub;
pub(crate) mod listen;
pub(crate) mod core;


use super::transport::Transport;

use crate::protocol::*;
use crate::stopper::Stopper;

use crate::err::*;
use crate::transport::memory::MemoryConnector;
use crate::transport::memory::MemoryListener;
use crate::transport::tcp::TcpListener;
use crate::transport::unix::UnixListener;

use std::path::PathBuf;
use listen::listen_and_serve;
use tokio::net::ToSocketAddrs;



pub(crate) enum Task {
    Register(Box<dyn Transport<ProtocolServer, ProtocolClient>>),
    Deregister(u32),
    Message(u32, Msg<ProtocolClient>),
    RequestTimeout(u32),
}

pub fn listen_and_serve_unix(addr: &PathBuf) -> BusResult<impl Stopper> {
    let listener = UnixListener::new(addr)?;
    listen_and_serve(listener)
}

pub async fn listen_and_serve_tcp(addr: impl ToSocketAddrs) -> BusResult<impl Stopper>{
    let listener = TcpListener::new(addr).await?;
    listen_and_serve(listener)
}

pub fn listen_and_serve_memory() -> BusResult<(impl Stopper, MemoryConnector)>{
    let (listener, connector) = MemoryListener::new();
    let stopper = listen_and_serve(listener)?;
    Ok((stopper, connector))
}


