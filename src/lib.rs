//! # T2 Bus
//! 
//! An inter- or intra-process message bus supporting publish/subscribe and request/response.  
//! 
//! ## Inter-process mode
//! 
//! The bus can operate as an inter-process message bus. In this case the server and each of the clients run in separate processes and messages are passed via a Unix socket.
//! 
//! The binary `t2_bus` can be used to run an inter-process bus at a specified unix socket address:
//! 
//! ```ignore
//! > cargo install --path .
//! > t2_bus my_bus
//! ```
//! 
//! A client can then be created in rust code:
//! 
//! ```
//! // connect a new client to the bus listening at "my_bus"
//! let (client, _joiner) = bus::Client::new_unix("my_bus").await.unwrap();
//! ```
//! 
//! ## Intra-process mode
//! 
//! Alternatively the bus can operate in the same process as the clients. In this case messages will be passed by a memory queue, which is significantly faster.
//! 
//! ```ignore
//! // start an in process bus service
//! let(mut listener, _stopper, _joiner) = bus::serve_bus_in_process().unwrap();
//! // connect a new client to the bus
//! let (client, _joiner) = bus::Client::new_memory(&mut listener).unwrap();
//! ```
//! 
//! ## Publish/Subscribe
//! 
//! Clients may subscribe to topics in order to receive all messages published on matching topics.
//! 
//! ```
//! use serde::{Deserialize, Serialize};
//! use bus::serve_bus_in_process;
//! use bus::Client;
//! use bus::PublishProtocol;
//! 
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
//! // Subscribe for all `HelloProtocol` type messages published on topics matching "alice"
//! let mut subscription = client_1.subscribe::<HelloProtocol>("alice").await?;
//! 
//! // Publish a `HelloProtocol` type message to all subscribers for topics matching "alice"
//! client_2.publish("alice", &HelloProtocol("Hello Alice".to_string())).await?;
//! 
//! // Wait to receive a message on this subscription
//! let (topic, message) = subscription.recv().await?;
//! 
//! assert_eq!(message.0, "Hello Alice".to_string());
//! 
//! // When the subscription object is dropped the subscription ends
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
//! ```ignore
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
//! 
//! // A client begins to serve the `HelloProtocol` at topic ''
//! let mut request_subscription = client_1.serve::<HelloRequest>("").await?;
//!
//! // Another client sends a HelloProtocol request on topic '' and later receives a response
//! let response = client_2.request("", &HelloRequest("Alice".to_string())).await?;
//! 
//! // The serving client receives the request...
//! let (_topic, request_id, request) = request_subscription.recv().await?;
//!
//! // ...and sends the response
//! client_1.respond::<HelloRequest>(request_id, &HelloResponse(format!("Hello {}", &request.0))).await?;
//! ```

pub(crate) mod client;
pub(crate) mod directory;
pub mod server;
pub mod topic;
pub(crate) mod transport;
pub(crate) mod protocol;
pub(crate) mod err;
#[cfg(debug_assertions)]
pub(crate) mod debug;
mod stopper;

#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate serde_derive;

#[cfg(test)]
pub mod test;

pub use err::*;
pub use protocol::{RequestProtocol, PublishProtocol, PubMsg};
pub use client::Client;

/// Start a bus server using the in-process memory transport. You can then connect to
/// and use the bus from within the same rust program.
/// ```rust
/// # fn main() -> crate::err::BusResult<()> {
///    let(_stopper, connector) = listen_and_serve_memory()?;
///
///    // Create and connect a client
///    let (mut client, _client_joiner) = Client::new_memory(&connector)?;
/// #
/// #     Ok(())
/// # }
/// ```
pub use server::listen_and_serve_memory;

/// Start a bus server using the unix socket transport. You can then connect to
/// and use the bus from within a separate process.
/// ```rust
/// # fn main() -> crate::err::BusResult<()> {
///    let _stopper = serve_bus_unix_socket("my_bus")?;
///
///    // Then is a separate process, connect a client to the bus:
///    let (mut client, _client_joiner) = Client::new_unix("my_bus")?;
/// #
/// #     Ok(())
/// # }
/// ```
pub use server::listen_and_serve_unix;
pub use server::listen_and_serve_tcp;
pub use transport::cbor_codec::CborCodec;
pub use client::subscription::{RequestSubscription, Subscription, SubscriptionInto};
pub use stopper::Stopper;

pub const DEFAULT_BUS_ADDR: &str = ".bus";



