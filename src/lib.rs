//! # T2 Bus
//! 
//! A message bus supporting publish/subscribe and request/response.  
//! 
//! The bus supports three transport types:
//! 
//! - Memory: messages are passed via a memory queue between clients that exist within the same process.
//! - Unix: Messages are passed via a Unix socket between clients that exist within the same or different processes on a single host machine.
//! - TCP: Messages are passed via a TCP socket between clients that may be on different hosts. Optionally, TLS may be used to provide encryption and mutual authentication between the client and server.
//! 
//! ## Unix Mode
//! 
//! The bus can operate as an inter-process message bus. In this case the server and each of the clients run in separate processes and messages are passed via a Unix socket.
//! 
//! The binary `t2_bus` can be used to run an inter-process bus at a specified unix socket address:
//! 
//! ```text
//! > cargo install --path .
//! > t2_bus my_bus
//! ```
//! 
//! A client can then be created in rust code:
//! 
//! ```
//! # use t2_bus::prelude::*;
//! # async fn connect_client() {
//! // connect a new client to the bus listening at "my_bus"
//! let client = t2_bus::transport::unix::connect(&"my_bus".into()).await.unwrap();
//! # }
//! ```
//! 
//! ## Memory Mode
//! 
//! Alternatively the bus can operate in the same process as the clients. In this case messages will be passed by a memory queue, which is significantly faster.
//! 
//! ```
//! # use t2_bus::prelude::*;
//! # fn test() {
//! // start an in process bus service
//! let (stopper, connector) = t2_bus::transport::memory::serve().unwrap();
//! // connect a new client to the bus
//! let client = t2_bus::transport::memory::connect(&connector).unwrap();
//! # 
//! # }
//! ```
//! ## TCP Mode
//! 
//! If the bus and the clients need to be on different hosts then TCP mode can used.
//! 
//! ```
//! # use t2_bus::prelude::*;
//! # async fn test() {
//! // start a TCP based bus
//! let stopper = t2_bus::transport::tcp::serve("localhost:4242").await.unwrap();
//! // connect a new client to the bus
//! let client = t2_bus::transport::tcp::connect("localhost:4242").await.unwrap();
//! # }
//! ```
//! 
//! ## Publish/Subscribe
//! 
//! Clients may subscribe to topics in order to receive all messages published on matching topics.
//! 
//! ```
//! # use serde::{Deserialize, Serialize};
//! # use t2_bus::prelude::*;
//! #
//! // Define a protocol message type
//! #[derive(Clone, Deserialize, Serialize, Debug)]
//! struct HelloProtocol(String);
//! 
//! // Specify that this is a pub/sub protocol
//! impl PublishProtocol for HelloProtocol{
//!     // Define a prefix to identify this protocol. This should be unique within all the protocols on your bus.
//!     fn prefix() -> &'static str {
//!         "hello"
//!     }
//! }
//! 
//! // ..
//! 
//! # async fn test() -> BusResult<()> {
//!     # let (stopper, connector) = t2_bus::transport::memory::serve()?;;
//!     # let client_1 = t2_bus::transport::memory::connect(&connector)?;
//!     # let client_2 = t2_bus::transport::memory::connect(&connector)?;
//!     # 
//! // Subscribe for all `HelloProtocol` type messages published on topics matching "alice"
//! let mut subscription = client_1.subscribe::<HelloProtocol>("alice").await?;
//! 
//! // Publish a `HelloProtocol` type message to all subscribers for topics matching "alice"
//! client_2.publish("alice", &HelloProtocol("Hello Alice".to_string())).await?;
//! 
//! // Wait to receive a message on this subscription
//! let (topic, message) = subscription.recv().await.unwrap();
//! 
//! assert_eq!(message.0, "Hello Alice".to_string());
//! 
//! // When the subscription object is dropped the subscription ends
//!     #
//!     # Ok(())
//! # }
//! #

//! ```
//! 
//! ## Topic Matching
//! 
//! Messages are routed according to topics. A topic is a string similar in form to a file system path. Here are some examples:
//! 
//! ```text
//! price
//! price/eth
//! price/eth/eur
//! price/*/eur
//! price/**
//! ```
//! 
//! The topic is composed of "fragments" separated by `/`. Each fragment is either a word composed of `a-z` and `_` or the wildcard `*` or the double wildcard `**`. 
//! 
//! Word fragments match identical word fragments. The wildcard `*` matches any fragment at the same position. The double wildcard `**` matches any fragment any number of times. 
//! 
//! Wildcards are supported in both subscribe topics and publish topics. 
//! 
//! To illustrate topic matching, consider an example chat app. If we define that messages from a `user` in a `room` are published to the topic `<room>/<user>` then:
//! 
//! - `alice` publishes a message in the `lobby` by publishing to `lobby/alice`.
//! - `alice` publishes a message in all rooms by publishing to `*/alice`
//! - `bob` subscribes to `alice`'s messages in the `lobby` by subscribing to `lobby/alice`
//! - `bob` subscribes to all messages in the `lobby` by subscribing to `lobby/*`
//! - `bob` subscribes to alice's messages in all rooms by subscribing to `*/alice`
//! - `bob` subscribes to all messages in all rooms by subscribing to `**`
//! 
//! ## Request/Response
//! 
//! Clients may begin to "serve" a topic. While serving a topic, requests on that protocol and topic will be routed exclusively 
//! to that client. The serving client should then produce a response which will be routed back to the origin of the request.
//! 
//! At most one client may serve a given topic at a time. However, it _is_ allowed to publish/subscribe on a topic while that 
//! topic is being served as the two systems do not interact.
//! 
//! Topic wildcards are not supported for either serving or requesting.
//! 
//! ```
//! # use t2_bus::prelude::*;
//! # use serde::{Serialize, Deserialize};
//! #
//! // Define protocol message types for request and response
//! #[derive(Clone, Deserialize, Serialize, Debug)]
//! struct HelloRequest(String);
//! 
//! #[derive(Clone, Deserialize, Serialize, Debug)]
//! struct HelloResponse(String);
//! 
//! // Specify that this is a req/rsp protocol
//! impl RequestProtocol for HelloRequest{
//!     type Rsp = HelloResponse;
//!     // Define a prefix to identify this protocol. This should be unique within all the protocols on your bus.
//!     fn prefix() -> &'static str {
//!         "hello"
//!     }
//! }
//! #
//! # #[tokio::main]
//! # async fn main() -> BusResult<()> {
//! #
//! # let (stopper, connector) = t2_bus::transport::memory::serve()?;
//! #
//! # let client_1 = t2_bus::transport::memory::connect(&connector)?;
//! # let client_2 = t2_bus::transport::memory::connect(&connector)?;
//! 
//! // A client begins to serve the `HelloProtocol` at topic ''
//! let mut request_subscription = client_1.serve::<HelloRequest>("").await?;
//!
//! # tokio::spawn(async move {
//! // Another client sends a HelloProtocol request on topic '' and later receives a response
//! let response = client_2.request("", &HelloRequest("Alice".to_string())).await.unwrap();
//! # });
//! 
//! // The serving client receives the request...
//! let (_topic, request_id, request) = request_subscription.recv().await.unwrap();
//!     
//! // ...and sends the response
//! client_1.respond::<HelloRequest>(request_id, &HelloResponse(format!("Hello {}", &request.0))).await?;
//! 
//! # Ok(())
//! 
//! # }
//! ```

pub mod prelude;
pub mod transport;

pub(crate) mod client;
pub(crate) mod directory;
pub(crate) mod server;
pub(crate) mod topic;
pub(crate) mod protocol;
pub(crate) mod err;

#[cfg(debug_assertions)]
mod debug;
mod stopper;

#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate serde_derive;

#[cfg(test)]
pub mod test;

pub const DEFAULT_BUS_ADDR: &str = ".bus";



