# T2 Bus

A message bus supporting publish/subscribe and request/response.  

The bus supports three transport types:

- Memory: messages are passed via a memory queue between clients that exist within the same process.
- Unix: Messages are passed via a Unix socket between clients that exist within the same or different processes on a single host machine.
- TCP: Messages are passed via a TCP socket between clients that may be on different hosts. Optionally, TLS may be used to provide encryption and mutual authentication between the client and server.

## Usage

### Start a bus server

```rust
use t2_bus::prelude::*;
async fn start_server() {
    let (server_stopper, memory_connector) = ServerBuilder::new()
        .serve_unix_socket("my_bus".parse().unwrap())
        .serve_memory()
        .serve_tcp("127.0.0.1:8000".parse().unwrap())
        .build()
        .await
        .unwrap();

    // The server is now listening for connections via the three specified endpoints.

    server_stopper.stop(); // or drop `server_stopper`

    // The server has stopped.
}
```

Alternatively, if you don't need an in-process bus, you may use the provided command line utility `t2` to start a TCP and/or Unix socket server:

```bash
cargo install --path .
t2 serve --unix my_bus --tcp 127.0.0.1:8000
```

### Connect to a server

```rust
use t2_bus::prelude::*;
async fn connect_to_a_server() {
    let (server_stopper, memory_connector) = ServerBuilder::new()
        .serve_unix_socket("my_bus".parse().unwrap())
        .serve_memory()
        .serve_tcp("127.0.0.1:8000".parse().unwrap())
        .build()
        .await
        .unwrap();

    let memory_client = memory_connector.unwrap().connect().await.unwrap();
    let unix_socket_client = Connector::new_unix("my_bus".parse().unwrap()).connect().await.unwrap();
    let tcp_client = Connector::new_tcp("127.0.0.1".parse().unwrap()).connect().await.unwrap();
}
```

### Publish/Subscribe

Clients may subscribe to topics in order to receive all messages published on matching topics.

```rust
use serde::{Deserialize, Serialize};
use t2_bus::prelude::*;

// Define a protocol message type
#[derive(Clone, Deserialize, Serialize, Debug)]
struct HelloProtocol(String);

// Specify that this is a pub/sub protocol
impl PublishProtocol for HelloProtocol{
    // Define a prefix to identify this protocol. This should be unique within all the protocols on your bus.
    fn prefix() -> &'static str {
        "hello"
    }
}

// ..

#[tokio::main]
async fn main() -> BusResult<()> {

    let (server_stopper, memory_connector) = ServerBuilder::new()
        .serve_memory()
        .build()
        .await?;

    let memory_connector = memory_connector.unwrap();
    
    let client_1 = memory_connector.connect().await?;
    let client_2 = memory_connector.connect().await?;

    // Subscribe for all `HelloProtocol` type messages published on topics matching "alice"
    let mut subscription = client_1.subscribe::<HelloProtocol>("alice").await?;

    // Publish a `HelloProtocol` type message to all subscribers for topics matching "alice"
    client_2.publish("alice", &HelloProtocol("Hello Alice".to_string())).await?;

    // Wait to receive a message on this subscription
    let (topic, message) = subscription.recv().await.unwrap();

    assert_eq!(message.0, "Hello Alice".to_string());

    // When the subscription object is dropped the subscription ends
    Ok(())
}

```

### Request/Response

Clients may begin to "serve" a topic. While serving a topic, requests on that protocol and topic will be routed exclusively 
to that client. The serving client should then produce a response which will be routed back to the origin of the request.

At most one client may serve a given topic at a time. However, it _is_ allowed to publish/subscribe on a topic while that 
topic is being served as the two systems do not interact.

Topic wildcards are not supported for either serving or requesting.

```rust
use t2_bus::prelude::*;
use serde::{Serialize, Deserialize};

// Define protocol message types for request and response
#[derive(Clone, Deserialize, Serialize, Debug)]
struct HelloRequest(String);

#[derive(Clone, Deserialize, Serialize, Debug)]
struct HelloResponse(String);

// Specify that this is a req/rsp protocol
impl RequestProtocol for HelloRequest{
    type Rsp = HelloResponse;
    // Define a prefix to identify this protocol. This should be unique within all the protocols on your bus.
    fn prefix() -> &'static str {
        "hello"
    }
}

#[tokio::main]
async fn main() -> BusResult<()> {

    let (server_stopper, memory_connector) = ServerBuilder::new()
        .serve_memory()
        .build()
        .await?;

    let memory_connector = memory_connector.unwrap();
    
    let client_1 = memory_connector.connect().await?;
    let client_2 = memory_connector.connect().await?;

    // A client begins to serve the `HelloProtocol` at topic ''
    let mut request_subscription = client_1.serve::<HelloRequest>("").await?;

    tokio::spawn(async move {
        // Another client sends a HelloProtocol request on topic '' and later receives a response
        let response = client_2.request("", &HelloRequest("Alice".to_string())).await.unwrap();
    });

    // The serving client receives the request...
    let (_topic, request_id, request) = request_subscription.recv().await.unwrap();
        
    // ...and sends the response
    client_1.respond::<HelloRequest>(request_id, &HelloResponse(format!("Hello {}", &request.0))).await?;

    Ok(())
}
```

## Topic Matching

Messages are routed according to topics. A topic is a string similar in form to a file system path. Here are some examples:

```text
price
price/eth
price/eth/eur
price/*/eur
price/**
```

The topic is composed of "fragments" separated by `/`. Each fragment is either a word composed of `a-z` and `_` or the wildcard `*` or the double wildcard `**`. 

Word fragments match identical word fragments. The wildcard `*` matches any fragment at the same position. The double wildcard `**` matches any fragment any number of times. 

Wildcards are supported in both subscribe topics and publish topics. 

To illustrate topic matching, consider an example chat app. If we define that messages from a `user` in a `room` are published to the topic `<room>/<user>` then:

- `alice` publishes a message in the `lobby` by publishing to `lobby/alice`.
- `alice` publishes a message in all rooms by publishing to `*/alice`
- `bob` subscribes to `alice`'s messages in the `lobby` by subscribing to `lobby/alice`
- `bob` subscribes to all messages in the `lobby` by subscribing to `lobby/*`
- `bob` subscribes to alice's messages in all rooms by subscribing to `*/alice`
- `bob` subscribes to all messages in all rooms by subscribing to `**`