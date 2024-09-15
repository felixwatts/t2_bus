use websocket::url::Url;

pub async fn serve(addr: impl ToSocketAddrs) -> BusResult<MultiStopper> {
    let listener = TcpListener::new(addr).await?;
    listen_and_serve(listener)
}

pub async fn connect (
    addr: &Url,
) -> BusResult<Client> {
    let socket = tokio::net::TcpStream::connect(addr).await?;
    let ws = websocket::client::builder::ClientBuilder::from_url(address).async_connect();
    Ok(socket.into())
}

fn build_tls_connector() -> TlsConnector {
    let mut root_cert_store = RootCertStore::empty();

    let mut pem = BufReader::new(File::open(ca_file)?);
    for cert in rustls_pemfile::certs(&mut pem) {
        root_cert_store.add(cert?).unwrap();
    }

    let cert_chain = load_certs(certs_pem_file)?;
    let key = load_key(key_file)?;

    let config = ClientConfig::builder()
        .with_root_certificates(root_cert_store)
        .with_client_auth_cert(cert_chain, key)
        .map_err(|e| BusError::TlsConfigError(e.to_string()))?;
    
    let connector = TlsConnector::from(Arc::new(config));
}