use std::convert::TryFrom;

use std::fs::File;
use std::io::BufReader;


use std::path::Path;
use std::sync::Arc;

use rustls_pemfile::certs;
use rustls_pemfile::private_key;
use tokio::net::{TcpStream, ToSocketAddrs};
use tokio_rustls::rustls::pki_types::CertificateDer;
use tokio_rustls::rustls::pki_types::PrivateKeyDer;
use tokio_rustls::rustls::pki_types::ServerName;

use tokio_rustls::rustls::ClientConfig;
use tokio_rustls::rustls::RootCertStore;
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;
use tokio_util::codec::Framed;

use crate::server::listen;
use crate::server::listen::listen_and_serve;
use crate::stopper::MultiStopper;
use crate::{protocol::{Msg, ProtocolClient, ProtocolServer}, server::listen::Listener, err::BusResult, transport::CborCodec};

use super::BusError;
use super::Transport;

pub async fn serve(addr: impl ToSocketAddrs, certs_pem_file: &Path, key_file: &Path) -> BusResult<MultiStopper> {
    let listener = TlsListener::new(addr, certs_pem_file, key_file).await?;
    listen_and_serve(listener)
}

pub async fn connect (
    host: &str,
    port: u16,
    ca_file: &Path
) -> BusResult<Framed<TlsStream<TcpStream>, CborCodec<Msg<ProtocolClient>, Msg<ProtocolServer>>>> {

    let mut root_cert_store = RootCertStore::empty();
    let mut pem = BufReader::new(File::open(ca_file)?);
    for cert in rustls_pemfile::certs(&mut pem) {
        root_cert_store.add(cert?).unwrap();
    }

    let config = ClientConfig::builder()
        .with_root_certificates(root_cert_store)
        .with_no_client_auth(); // i guess this was previously the default?
    
    let connector = TlsConnector::from(Arc::new(config));

    let socket = TcpStream::connect(&format!("{host}:{port}")).await?;

    let domain = ServerName::try_from(host)
        .map_err(|_| BusError::InternalError("invalid hostname".into()))?
        .to_owned();

    let tls_socket = connector.connect(domain, socket).await?;

    let transport = tokio_util::codec::Framed::new(tls_socket, CborCodec::new());
    Ok(transport)
}

struct TlsListener{
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