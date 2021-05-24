use crate::err::*;

use super::protocol::Msg;

pub mod cbor_codec;
pub mod memory_transport;
pub mod unix_socket_transport;

pub trait Transport<TSend, TRecv>:
    'static
    + futures::Stream<Item = BusResult<Msg<TRecv>>>
    + futures::Sink<Msg<TSend>, Error = BusError>
    + Send
    + Unpin
{
}
