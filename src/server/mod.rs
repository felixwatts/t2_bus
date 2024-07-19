mod client_stub;
pub(crate) mod listen;
pub(crate) mod core;


use super::transport::Transport;

use crate::protocol::*;
use crate::stopper::Stopper;

use crate::err::*;
use crate::transport::memory::MemoryConnector;
use crate::transport::memory::MemoryListener;
use crate::transport::tcp::TcpListener;
use crate::transport::unix::UnixListener;

use std::path::PathBuf;
use listen::listen_and_serve;
use tokio::net::ToSocketAddrs;



pub(crate) enum Task {
    Register(Box<dyn Transport<ProtocolServer, ProtocolClient>>),
    Deregister(u32),
    Message(u32, Msg<ProtocolClient>),
    RequestTimeout(u32),
}

/// Start a bus server using the unix socket transport. You can then connect to
/// and use the bus from within a separate process.
/// ```rust
/// # use t2_bus::prelude::*;
/// # async fn test() -> BusResult<()> {
///    let stopper = listen_and_serve_unix(&"my_bus".into())?;
///
///    // Then in a separate process, connect a client to the bus:
///    let client = Client::new_unix(&"my_bus".into()).await?;
/// #
/// #     Ok(())
/// # }
/// ```
pub fn listen_and_serve_unix(addr: &PathBuf) -> BusResult<impl Stopper> {
    let listener = UnixListener::new(addr)?;
    listen_and_serve(listener)
}

/// Start a bus server using the TCP socket transport. You can then connect to
/// and use the bus from within a separate host.
/// ```rust
/// # use t2_bus::prelude::*;
/// # async fn test() -> BusResult<()> {
///    let stopper = listen_and_serve_tcp("localhost:4242").await?;
///
///    // Then connect a client to the bus:
///    let client = Client::new_tcp("localhost:4242").await?;
/// #
/// #     Ok(())
/// # }
/// ```
pub async fn listen_and_serve_tcp(addr: impl ToSocketAddrs) -> BusResult<impl Stopper>{
    let listener = TcpListener::new(addr).await?;
    listen_and_serve(listener)
}

/// Start a bus server using the in-process memory transport. You can then connect to
/// and use the bus from within the same rust program.
/// ```rust
/// # use t2_bus::prelude::*;
/// # #[tokio::main]
/// # async fn main() -> BusResult<()> {
///    let(_stopper, connector) = listen_and_serve_memory()?;
///
///    // Create and connect a client
///    let client = Client::new_memory(&connector)?;
/// #
/// #     Ok(())
/// # }
/// ```
pub fn listen_and_serve_memory() -> BusResult<(impl Stopper, MemoryConnector)>{
    let (listener, connector) = MemoryListener::new();
    let stopper = listen_and_serve(listener)?;
    Ok((stopper, connector))
}


