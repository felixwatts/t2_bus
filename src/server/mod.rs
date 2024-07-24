mod client_stub;
pub(crate) mod listen;
pub(crate) mod core;

use super::transport::Transport;
use crate::protocol::*;

pub(crate) enum Task {
    Register(Box<dyn Transport<ProtocolServer, ProtocolClient>>),
    Deregister(u32),
    Message(u32, Msg<ProtocolClient>),
    RequestTimeout(u32),
}