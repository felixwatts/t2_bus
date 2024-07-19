use crate::{
    client::Client, err::BusResult, prelude::Stopper, protocol::{Msg, ProtocolClient, ProtocolServer}, server::listen::{listen_and_serve, Listener}, stopper::MultiStopper
};
use std::path::PathBuf;
use tokio::net::UnixStream;
use tokio_util::codec::Framed;
use super::{cbor_codec::CborCodec, Transport};

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
pub fn serve(addr: &PathBuf) -> BusResult<impl Stopper> {
    let listener = UnixListener::new(addr)?;
    listen_and_serve(listener)
}

pub async fn connect(
    addr: &PathBuf,
) -> BusResult<Client> {
    let socket = UnixStream::connect(addr).await?;
    let transport = tokio_util::codec::Framed::new(socket, CborCodec::new());
    let client = Client::new(transport)?;
    Ok(client)
}

pub(crate) struct UnixListener(tokio::net::UnixListener);

impl UnixListener{
    pub(crate) fn new(addr: &PathBuf) -> BusResult<Self>{
        let _ = std::fs::remove_file(addr);
        let listener = tokio::net::UnixListener::bind(addr)?;
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