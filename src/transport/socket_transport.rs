use crate::{
    err::BusResult, protocol::{Msg, ProtocolClient, ProtocolServer, ServerCodec}, server::{core::Core, listen::{self, Listener}, Task}
};
use std::{path::PathBuf};

use futures::{AsyncRead, AsyncWrite};
use tokio::{
    net::{TcpSocket, TcpStream, ToSocketAddrs, UnixStream},
    sync::mpsc::UnboundedSender,
    task::JoinHandle,
};
use tokio_util::codec::Framed;

use super::{cbor_codec::CborCodec, Transport};

pub(crate) async fn connect_unix(
    addr: &PathBuf,
) -> BusResult<Framed<UnixStream, CborCodec<Msg<ProtocolClient>, Msg<ProtocolServer>>>> {
    let socket = UnixStream::connect(addr).await?;
    let transport = tokio_util::codec::Framed::new(socket, CborCodec::new());
    Ok(transport)
}

pub(crate) async fn connect_tcp<A: ToSocketAddrs> (
    addr: A,
) -> BusResult<Framed<TcpStream, CborCodec<Msg<ProtocolClient>, Msg<ProtocolServer>>>> {
    let socket = tokio::net::TcpStream::connect(addr).await?;
    let transport = tokio_util::codec::Framed::new(socket, CborCodec::new());
    Ok(transport)
}

pub(crate) struct UnixListener(tokio::net::UnixListener);
pub(crate) struct TcpListener(tokio::net::TcpListener);

impl UnixListener{
    pub(crate) fn new(addr: &PathBuf) -> BusResult<Self>{
        let _ = std::fs::remove_file(addr);
        let listener = tokio::net::UnixListener::bind(addr)?;
        Ok(
            Self(listener)
        )
    }
}

impl TcpListener{
    pub(crate) async fn new(addr: impl ToSocketAddrs) -> BusResult<Self>{
        let listener = tokio::net::TcpListener::bind(addr).await?;
        Ok(
            Self(listener)
        )
    }
}

impl Listener for UnixListener{
    async fn accept(&mut self) -> BusResult<impl Transport<ProtocolServer, ProtocolClient>> {
        let (socket, _) = self.0.accept().await?;
        let transport = tokio_util::codec::Framed::new(socket, CborCodec::new());
        Ok(transport)
    }
}

impl Listener for TcpListener{
    async fn accept(&mut self) -> BusResult<impl Transport<ProtocolServer, ProtocolClient>> {
        let (socket, _) = self.0.accept().await?;
        let transport = tokio_util::codec::Framed::new(socket, CborCodec::new());
        Ok(transport)
    }
}

impl<TSend, TRecv, TSocket> Transport<TSend, TRecv>
    for tokio_util::codec::Framed<TSocket, CborCodec<Msg<TSend>, Msg<TRecv>>>
where
    TSend: 'static + serde::Serialize + Send,
    TRecv: 'static + serde::de::DeserializeOwned + Send,
    TSocket: 'static + Send + Unpin + tokio::io::AsyncWrite + tokio::io::AsyncRead
{
}
