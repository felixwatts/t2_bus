use serde::{Deserialize, Serialize};
use t2_bus::{listen_and_serve_memory, BusResult, Client, RequestProtocol};

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
    let(_stopper,  mut connector) = listen_and_serve_memory()?;

    // Create and connect two clients
    let (mut requester, _requester_joiner) = Client::new_memory(&mut connector)?;
    let (mut responder, _responder_joiner) = Client::new_memory(&mut connector)?;

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