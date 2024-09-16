use std::time::Duration;
use tokio::net::{lookup_host, ToSocketAddrs};
use crate::client::Client;
use crate::server::listen::listen_and_serve;
use crate::stopper::MultiStopper;
use crate::{protocol::{ProtocolClient, ProtocolServer}, server::listen::Listener, err::BusResult, transport::CborCodec};

use super::{BusError, Transport};

/// Start a bus server using the TCP socket transport. You can then connect to
/// and use the bus from within a separate host.
/// ```rust
/// # use t2_bus::prelude::*;
/// # async fn test() -> BusResult<()> {
///    let stopper = t2_bus::transport::tcp::serve("localhost:4242").await?;
///
///    // Then connect a client to the bus:
///    let client = t2_bus::transport::tcp::connect("localhost:4242").await?;
/// #
/// #     Ok(())
/// # }
/// ```
pub async fn serve(addr: impl ToSocketAddrs) -> BusResult<MultiStopper> {
    let listener = TcpListener::new(addr).await?;
    listen_and_serve(listener)
}

pub async fn connect (
    addr: impl ToSocketAddrs,
) -> BusResult<Client> {
    let socket = tokio::net::TcpSocket::new_v4()?;
    socket.set_keepalive(true)?;
    let addr = lookup_host(addr).await?.into_iter().next().ok_or(BusError::DnsLookupFailed)?;
    let stream = socket.connect(addr).await?;
    let transport = tokio_util::codec::Framed::new(stream, CborCodec::new());
    let client = Client::new(transport)?;
    Ok(client)
}

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
