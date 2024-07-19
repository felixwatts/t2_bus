use crate::{err::*, CborCodec};

use super::protocol::Msg;

pub mod cbor_codec;
pub mod memory;
pub mod unix;
pub mod tcp;

pub trait Transport<TSend, TRecv>:
    'static
    + futures::Stream<Item = BusResult<Msg<TRecv>>>
    + futures::Sink<Msg<TSend>, Error = BusError>
    + Send
    + Unpin
{
}

impl<TSend, TRecv, TSocket> Transport<TSend, TRecv>
    for tokio_util::codec::Framed<TSocket, CborCodec<Msg<TSend>, Msg<TRecv>>>
where
    TSend: 'static + serde::Serialize + Send,
    TRecv: 'static + serde::de::DeserializeOwned + Send,
    TSocket: 'static + Send + Unpin + tokio::io::AsyncWrite + tokio::io::AsyncRead
{
}
