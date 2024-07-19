use crate::{
    err::BusResult, protocol::{Msg, ProtocolClient, ProtocolServer}, server::{listen::{Listener}}
};
use std::{path::PathBuf};


use tokio::{
    net::{UnixStream},
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