use std::fs::File;
use std::io::BufReader;
use std::path;
use std::path::Path;
use std::sync::Arc;

use rustls_pemfile::certs;
use rustls_pemfile::private_key;
use tokio::net::{TcpStream, ToSocketAddrs};
use tokio_rustls::rustls::pki_types::CertificateDer;
use tokio_rustls::rustls::pki_types::PrivateKeyDer;
use tokio_rustls::rustls::server::danger::ClientCertVerifier;
use tokio_rustls::rustls::ClientConfig;
use tokio_rustls::rustls::RootCertStore;
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::TlsAcceptor;
use tokio_util::codec::Framed;

use crate::{protocol::{Msg, ProtocolClient, ProtocolServer}, server::listen::Listener, err::BusResult, transport::CborCodec};

use super::BusError;
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

pub(crate) struct TlsListener{
    listener: tokio::net::TcpListener,
    tls_acceptor: TlsAcceptor
}

impl TlsListener{
    pub(crate) async fn new(addr: impl ToSocketAddrs, certs_pem_file: &Path, key_file: &Path) -> BusResult<Self>{
        let listener = tokio::net::TcpListener::bind(addr).await?;
        let certs = TlsListener::load_certs(certs_pem_file)?;
        let key = TlsListener::load_key(key_file)?;

        let config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .map_err(|err| BusError::InternalError(err.to_string()))?;

        let tls_acceptor = TlsAcceptor::from(Arc::new(config));

        Ok(
            Self{
                listener,
                tls_acceptor
            }
        )
    }
}

impl TlsListener{
    fn load_certs(path: &Path) -> BusResult<Vec<CertificateDer<'static>>> {
        certs(&mut BufReader::new(File::open(path)?)).map(|r| r.map_err(|e| BusError::InternalError(e.to_string()))).collect()
    }
    
    fn load_key(path: &Path) -> BusResult<PrivateKeyDer<'static>> {
        Ok(private_key(&mut BufReader::new(File::open(path)?))
            .unwrap()
            .ok_or(BusError::InternalError(
                "no private key found".to_string(),
            ))?)
    }
}

impl Listener for TlsListener{
    async fn accept(&mut self) -> BusResult<impl Transport<ProtocolServer, ProtocolClient>> {
        let (socket, _) = self.listener.accept().await?;
        let tls_acceptor = self.tls_acceptor.clone();
        let tls_socket = tls_acceptor.accept(socket).await?;
        let transport = tokio_util::codec::Framed::new(tls_socket, CborCodec::new());
        
        Ok(transport)
    }
}


