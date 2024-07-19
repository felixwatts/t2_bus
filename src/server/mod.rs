mod client_stub;
pub(crate) mod listen;
pub(crate) mod core;

use self::client_stub::*;
use super::transport::Transport;
use crate::directory::*;
use crate::protocol::*;
use crate::stopper::Stopper;
use crate::topic::*;
use crate::err::*;
use crate::transport::memory_transport::MemoryConnector;
use crate::transport::memory_transport::MemoryListener;
use crate::transport::socket_transport::UnixListener;
use std::collections::HashMap;
use std::path::PathBuf;
use listen::listen_and_serve;
use tokio::time::Duration;
use tokio::{
    sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
    task::JoinHandle,
};

pub(crate) enum Task {
    Register(Box<dyn Transport<ProtocolServer, ProtocolClient>>),
    Deregister(u32),
    Message(u32, Msg<ProtocolClient>),
    RequestTimeout(u32),
}

pub fn listen_and_serve_unix(addr: &PathBuf) -> BusResult<impl Stopper> {
    let listener = UnixListener::new(addr)?;
    Ok(listen_and_serve(listener)?)
}

pub fn listen_and_serve_tcp(){}

pub fn listen_and_serve_memory() -> BusResult<(impl Stopper, MemoryConnector)>{
    let (listener, connector) = MemoryListener::new();
    let stopper = listen_and_serve(listener)?;
    Ok((stopper, connector))
}


