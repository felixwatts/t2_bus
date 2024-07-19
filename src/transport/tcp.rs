use tokio::net::{TcpStream, ToSocketAddrs};
use tokio_util::codec::Framed;

use crate::{protocol::{Msg, ProtocolClient, ProtocolServer}, server::listen::Listener, err::BusResult, transport::CborCodec};

use super::Transport;

pub(crate) struct TcpListener(tokio::net::TcpListener);

impl TcpListener{
    pub(crate) async fn new(addr: impl ToSocketAddrs) -> BusResult<Self>{
        let listener = tokio::net::TcpListener::bind(addr).await?;
        Ok(
            Self(listener)
        )
    }
}

impl Listener for TcpListener{
    async fn accept(&mut self) -> BusResult<impl Transport<ProtocolServer, ProtocolClient>> {
        let (socket, _) = self.0.accept().await?;
        let transport = tokio_util::codec::Framed::new(socket, CborCodec::new());
        Ok(transport)
    }
}

pub(crate) async fn connect_tcp (
    addr: impl ToSocketAddrs,
) -> BusResult<Framed<TcpStream, CborCodec<Msg<ProtocolClient>, Msg<ProtocolServer>>>> {
    let socket = tokio::net::TcpStream::connect(addr).await?;
    let transport = tokio_util::codec::Framed::new(socket, CborCodec::new());
    Ok(transport)
}