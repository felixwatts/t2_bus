use std::time::Instant;

use futures::{stream::FuturesUnordered, StreamExt};
use tokio::{join, task::JoinHandle};

use super::{
    protocol::{PublishProtocol, RequestProtocol}
};
use crate::{client::Client, err::BusResult, stopper::Stopper};

#[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
struct TestPub(pub String);

impl PublishProtocol for TestPub {
    fn prefix() -> &'static str {
        "test"
    }
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
struct TestReq(pub String);

#[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
struct TestRsp(pub String);

impl RequestProtocol for TestReq {
    type Rsp = TestRsp;
    fn prefix() -> &'static str {
        "test"
    }
}

pub fn unique_addr() -> std::path::PathBuf {
    std::path::PathBuf::from(format!("/tmp/{}", uuid::Uuid::new_v4()))
}

async fn setup() -> (Client, Client, impl Stopper) {
    let addr = unique_addr();
    let listener = crate::transport::socket_transport::UnixListener::new(&addr).unwrap();
    let stopper = crate::server::listen::listen_and_serve(listener).unwrap();

    let (client_1, _) = Client::new_unix(&addr).await.unwrap();
    let (client_2, _) = Client::new_unix(&addr).await.unwrap();

    (client_1, client_2, stopper)
}

async fn setup_local() -> (
    Client, 
    Client, 
    impl Stopper
) {
    let (stopper, connector) = crate::server::listen_and_serve_memory().unwrap();

    let (client_1, _) = Client::new_memory(&connector).unwrap();
    let (client_2, _) = Client::new_memory(&connector).unwrap();

    (client_1, client_2, stopper)
}

#[tokio::test]
async fn test_subscribe_publish_local() {
    let (mut client_1, mut client_2, stopper) = setup_local().await;

    let mut rx = client_1.subscribe::<TestPub>("a/b/c").await.unwrap();

    client_2
        .publish("a/b/c", &TestPub("d".into()))
        .await
        .unwrap();
    let (topic, payload) = rx.recv().await.unwrap();

    assert_eq!("test/a/b/c", &topic);
    assert_eq!(&"d", &payload.0);

    stopper.stop().await.unwrap();
}

#[tokio::test]
async fn test_subscribe_publish() {
    let (mut client_1, mut client_2, _stopper) = setup().await;

    let mut rx = client_1.subscribe::<TestPub>("a/b/c").await.unwrap();

    client_2
        .publish("a/b/c", &TestPub("d".into()))
        .await
        .unwrap();
    let (topic, payload) = rx.recv().await.unwrap();

    assert_eq!("test/a/b/c", &topic);
    assert_eq!("d", payload.0);
}

#[tokio::test]
async fn test_subscribe_publish_empty_topic() {
    let (mut client_1, mut client_2, _stopper) = setup().await;

    let mut rx = client_1.subscribe::<TestPub>("").await.unwrap();

    client_2.publish("", &TestPub("d".into())).await.unwrap();
    let (topic, payload) = rx.recv().await.unwrap();

    assert_eq!("test", &topic);
    assert_eq!("d", payload.0);
}

#[tokio::test]
async fn test_unsubscribe() {
    let (mut client_1, mut client_2, _stopper) = setup().await;

    {
        let _subscription = client_1.subscribe::<TestPub>("a/b/c").await.unwrap();
        // receiver goes out of scope - unsubscribe
    }

    // new subscription to the topic
    let mut subscription_2 = client_1.subscribe::<TestPub>("a/b/c").await.unwrap();

    // client receives new value on topic
    client_2
        .publish("a/b/c", &TestPub("e".into()))
        .await
        .unwrap();

    // callback is called with correct value
    let (topic, payload) = subscription_2.recv().await.unwrap();
    assert_eq!("test/a/b/c", &topic);
    assert_eq!("e", payload.0);
}

#[tokio::test]
async fn test_unsubscribe_2() {
    let (mut client_1, mut client_2, _stopper) = setup().await;

    {
        let _ = client_1.subscribe::<TestPub>("a/b/c").await.unwrap();
        // receiver goes out of scope - unsubscribe
    }

    // new subscription to the topic
    let mut rx = client_1.subscribe::<TestPub>("a/b/c").await.unwrap();

    // client receives message on unsubscribed topic
    // cleanup of old subscription
    client_2
        .publish("a/b/c", &TestPub("d".into()))
        .await
        .unwrap();

    // callback of new subscription is called with correct value
    let (topic, payload) = rx.recv().await.unwrap();
    assert_eq!("test/a/b/c", &topic);
    assert_eq!("d", payload.0);
}

#[tokio::test]
async fn test_request_response() {
    let (mut client_1, mut client_2, _stopper) = setup().await;

    let mut rx = client_1.serve::<TestReq>("ping").await.unwrap();

    let ping_task = tokio::spawn(async move {
        let (topic, req_id, req) = rx.recv().await.unwrap();

        assert_eq!("test/ping", &topic);
        assert_eq!("PING", req.0);

        client_1
            .respond::<TestReq>(req_id, &TestRsp("PONG".into()))
            .await
            .unwrap();
    });

    let rsp_payload = client_2
        .request("ping", &TestReq("PING".into()))
        .await
        .unwrap();

    assert_eq!("PONG", rsp_payload.0);

    let _ = tokio::try_join!(ping_task).unwrap();
}

#[tokio::test]
async fn test_unserve() {
    let (mut client_1, mut client_2, _stopper) = setup().await;

    {
        let _request_subscription = client_2.serve::<TestReq>("ping").await.unwrap();
        // request subscription goes out of scope, client stops serving the topic
    }

    // now another client can serve the topic
    let mut request_subscription = client_1.serve::<TestReq>("ping").await.unwrap();

    let ping_task = tokio::spawn(async move {
        let (topic, req_id, req) = request_subscription.recv().await.unwrap();

        assert_eq!("test/ping", &topic);
        assert_eq!("PING", req.0);

        client_1
            .respond::<TestReq>(req_id, &TestRsp("PONG".into()))
            .await
            .unwrap();
    });

    let rsp_payload = client_2
        .request("ping", &TestReq("PING".into()))
        .await
        .unwrap();

    assert_eq!("PONG", rsp_payload.0);

    let _ = tokio::try_join!(ping_task).unwrap();
}

#[tokio::test]
async fn stress_test_pub_sub() {
    let start = Instant::now();

    // let mut core = Core::new();
    let listener = crate::transport::socket_transport::UnixListener::new(&".test".into()).unwrap();
    let stopper = crate::server::listen::listen_and_serve(listener).unwrap();
    // let _core_join_handle = core.spawn().unwrap();

    let mut client_join_handles = FuturesUnordered::new();

    async fn run_client(client_id: i32) -> BusResult<i32> {
        let topic = String::new();

        let (mut client, _client_join_handle) = crate::client::Client::new_unix(&".test".into()).await?;
        let mut sub = client.subscribe::<TestPub>(&topic).await?;

        let a: JoinHandle<BusResult<()>> = tokio::spawn(async move {
            let topic = String::new();
            for msg_id in 0..1u32 {
                let msg = &TestPub(format!("{}:{}", client_id, msg_id));
                client.publish(&topic, msg).await?;
            }
            Ok(())
        });

        let b: JoinHandle<BusResult<()>> = tokio::spawn(async move {
            for _ in 0..100u32 {
                sub.recv().await.unwrap();
            }
            Ok(())
        });

        let c = join!(a, b);

        c.0??;
        c.1??;

        Ok(client_id)
    }

    for client_id in 0..100 {
        client_join_handles.push(run_client(client_id));
    }

    loop {
        match client_join_handles.next().await {
            Some(result) => {
                result.unwrap();
            }
            None => break,
        }
    }

    let end = Instant::now();
    let duration = end - start;

    stopper.stop().await.unwrap();

    println!("{}", duration.as_millis());
}

#[tokio::test]
async fn stress_test_pub_sub_memory() {
    let start = Instant::now();

    let (stopper, connector) = crate::server::listen_and_serve_memory().unwrap();

    let mut client_join_handles = FuturesUnordered::new();

    async fn run_client(
        client_id: i32,
        mut client: Client
    ) -> BusResult<()> {
        let topic = String::new();
        let mut sub = client.subscribe::<TestPub>(&topic).await?;

        let a: JoinHandle<BusResult<()>> = tokio::spawn(async move {
            let topic = String::new();
            for msg_id in 0..100u32 {
                let msg = &TestPub(format!("{}:{}", client_id, msg_id));
                client.publish(&topic, msg).await?;
            }
            Ok(())
        });

        let b: JoinHandle<BusResult<()>> = tokio::spawn(async move {
            for _ in 0..100u32 {
                sub.recv().await.unwrap();
            }
            Ok(())
        });

        let c = join!(a, b);

        c.0??;
        c.1??;

        Ok(())
    }

    for client_id in 0..100 {
        let (client, _) = crate::client::Client::new_memory(&connector).unwrap();
        client_join_handles.push(run_client(client_id, client));
    }

    loop {
        match client_join_handles.next().await {
            Some(result) => {
                result.unwrap();
            }
            None => break,
        }
    }

    let end = Instant::now();
    let duration = end - start;

    println!("{}", duration.as_millis());

    stopper.stop().await.unwrap();
}

#[tokio::test]
async fn test_compression() {
    let sent = vec![42u8; 10000];
    let (mut client_1, mut client_2, _stopper) = setup().await;

    let mut sub = client_2.subscribe_bytes("topic").await.unwrap();
    client_1.publish_bytes("topic", sent).await.unwrap();

    let received: Vec<u8> = sub.recv().await.unwrap().payload.into();
    assert_eq!(received.len(), 10000);
    for byte in received {
        assert_eq!(byte, 42u8);
    }
}
