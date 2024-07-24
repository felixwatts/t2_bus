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

#[tokio::main]
async fn main() -> BusResult<()> {
    // Start a bus server using the in-process memory transport
    let(stopper, connector) = t2_bus::transport::memory::serve()?;

    // Create and connect two clients
    let publisher = t2_bus::transport::memory::connect(&connector)?;
    let subscriber = t2_bus::transport::memory::connect(&connector)?;

    // Subscriber subscribes to `HelloProtocol` protocol and 'alice' topic
    let mut subscription = subscriber.subscribe::<HelloProtocol>("alice").await?;

    // Publisher publishes a HelloProtocol message on topic 'alice'
    publisher.publish("alice", &HelloProtocol("Hello Alice".to_string())).await?;

    // Subscriber receives published message
    let (topic, message) = subscription.recv().await.unwrap();

    assert_eq!(topic, "hello/alice".to_string());
    assert_eq!(message.0, "Hello Alice".to_string());

    // When the `subscription` object is dropped the subscription ends

    stopper.stop().await.unwrap();

    Ok(())
}