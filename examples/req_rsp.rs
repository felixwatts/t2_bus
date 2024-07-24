use serde::{Deserialize, Serialize};
use t2_bus::prelude::*;

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
    // Start a bus server using the in-process memory transport
    let(_stopper,  connector) = t2_bus::transport::memory::serve()?;

    // Create and connect two clients
    let requester = t2_bus::transport::memory::connect(&connector)?;
    let responder = t2_bus::transport::memory::connect(&connector)?;

    // Service provider begins to serve the `HelloRequest` protocol at topic ''
    let mut request_subscription = responder.serve::<HelloRequest>("").await?;

    tokio::spawn(async move {
        // Service provider receives request
        let (_topic, request_id, request) = request_subscription.recv().await.unwrap();
        // Service provider sends response
        responder.respond::<HelloRequest>(request_id, &HelloResponse(format!("Hello {}", &request.0))).await?;
        BusResult::Ok(())
    });

    // Requester sends a HelloRequest request on topic 'alice'
    let response = requester.request("", &HelloRequest("Alice".to_string())).await?;

    assert_eq!(response.0, "Hello Alice".to_string());

    Ok(())
}