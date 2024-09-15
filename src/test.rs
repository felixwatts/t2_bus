use std::time::{Duration, Instant};
use futures::{stream::FuturesUnordered, StreamExt};
use tokio::{join, task::JoinHandle};
use super::protocol::{PublishProtocol, RequestProtocol};
use crate::{client::Client, err::{BusError, BusResult}, prelude::ServerBuilder, stopper::Stopper};

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

pub(crate) fn unique_addr() -> std::path::PathBuf {
    std::path::PathBuf::from(format!("/tmp/{}", uuid::Uuid::new_v4()))
}

async fn setup() -> (Client, Client, impl Stopper) {
    let addr = unique_addr();
    let stopper = crate::transport::unix::serve(&addr).unwrap();

    let client_1 = crate::transport::unix::connect(&addr).await.unwrap();
    let client_2 = crate::transport::unix::connect(&addr).await.unwrap();

    (client_1, client_2, stopper)
}

async fn setup_tcp() -> (Client, Client, impl Stopper) {
    let addr = "localhost:8999";
    let stopper = crate::transport::tcp::serve(addr).await.unwrap();

    let client_1 = crate::transport::tcp::connect(addr).await.unwrap();
    let client_2 = crate::transport::tcp::connect(addr).await.unwrap();

    (client_1, client_2, stopper)
}

async fn setup_local() -> (
    Client, 
    Client, 
    impl Stopper
) {
    let (stopper, connector) = crate::transport::memory::serve().unwrap();

    let client_1 = crate::transport::memory::connect(&connector).unwrap();
    let client_2 = crate::transport::memory::connect(&connector).unwrap();

    (client_1, client_2, stopper)
}

#[tokio::test]
async fn test_tcp() {
    let (client_1, client_2, stopper) = setup_tcp().await;

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
async fn test_subscribe_publish_local() {
    let (client_1, client_2, stopper) = setup_local().await;

    let mut rx = client_1.subscribe::<TestPub>("a/b/c").await.unwrap();

    client_2
        .publish("a/b/c", &TestPub("d".into()))
        .await
        .unwrap();
    let (topic, payload) = rx.recv().await.unwrap();

    assert_eq!("test/a/b/c", &topic);
    assert_eq!(&"d", &payload.0);

    stopper.stop().await.expect_err("Expect channel closed");
}

#[tokio::test]
async fn test_subscribe_publish() {
    let (client_1, client_2, _stopper) = setup().await;

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
    let (client_1, client_2, _stopper) = setup().await;

    let mut rx = client_1.subscribe::<TestPub>("").await.unwrap();

    client_2.publish("", &TestPub("d".into())).await.unwrap();
    let (topic, payload) = rx.recv().await.unwrap();

    assert_eq!("test", &topic);
    assert_eq!("d", payload.0);
}

#[tokio::test]
async fn test_unsubscribe() {
    let (client_1, client_2, _stopper) = setup().await;

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
    let (client_1, client_2, _stopper) = setup().await;

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
    let (client_1, client_2, _stopper) = setup().await;

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
    let (client_1, client_2, _stopper) = setup().await;

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
    let listener = crate::transport::unix::UnixListener::new(&".test".into()).unwrap();
    let stopper = crate::server::listen::listen_and_serve(listener).unwrap();
    // let _core_join_handle = core.spawn().unwrap();

    let mut client_join_handles = FuturesUnordered::new();

    async fn run_client(client_id: i32) -> BusResult<i32> {
        let topic = String::new();

        let client = crate::transport::unix::connect(&".test".into()).await?;
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

    while let Some(result) = client_join_handles.next().await {
        result.unwrap();
    }

    let end = Instant::now();
    let duration = end - start;

    stopper.stop().await.unwrap();

    println!("{}", duration.as_millis());
}

#[tokio::test]
async fn stress_test_pub_sub_memory() {
    let start = Instant::now();

    let (stopper, connector) = crate::transport::memory::serve().unwrap();

    let mut client_join_handles = FuturesUnordered::new();

    async fn run_client(
        client_id: i32,
        client: Client
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
        let client = crate::transport::memory::connect(&connector).unwrap();
        client_join_handles.push(run_client(client_id, client));
    }

    while let Some(result) = client_join_handles.next().await {
        result.unwrap();
    }

    let end = Instant::now();
    let duration = end - start;

    println!("{}", duration.as_millis());

    stopper.stop().await.unwrap();
}

#[tokio::test]
async fn test_compression() {
    let sent = vec![42u8; 10000];
    let (client_1, client_2, _stopper) = setup().await;

    let mut sub = client_2.subscribe_bytes("topic").await.unwrap();
    client_1.publish_bytes("topic", sent).await.unwrap();

    let received: Vec<u8> = sub.recv().await.unwrap().payload.into();
    assert_eq!(received.len(), 10000);
    for byte in received {
        assert_eq!(byte, 42u8);
    }
}

#[tokio::test]
async fn test_respond_with_bad_request_id() {
    let (client_1, client_2, _stopper) = setup().await;

    let mut rx = client_1.serve::<TestReq>("ping").await.unwrap();

    let ping_task = tokio::spawn(async move {
        let (topic, req_id, req) = rx.recv().await.unwrap();

        assert_eq!("test/ping", &topic);
        assert_eq!("PING", req.0);

        let result = client_1
            .respond::<TestReq>(req_id + 1, &TestRsp("PONG".into()))
            .await;

        assert_eq!(BusResult::Err(BusError::RequestFailed("Respond failed: Invalid request ID".into())), result);
    });

    let _ = tokio::time::timeout(
        Duration::from_secs(3), 
        client_2.request("ping", &TestReq("PING".into()))
    ).await;

    // assert_eq!(BusResult::Err(BusError::RequestFailedTimeout), rsp_result);

    let _ = tokio::try_join!(ping_task).unwrap();
}

#[tokio::test]
async fn test_multi_transport(){
    let unix_addr = crate::test::unique_addr();
    let tcp_addr = "localhost:8445";

    let (stopper, memory_connector) = ServerBuilder::new()
        .serve_memory().await.unwrap()
        .serve_unix_socket(&unix_addr).unwrap()
        .serve_tcp(tcp_addr).await.unwrap()
        .build()
        .unwrap();

    let memory_client = crate::transport::memory::connect(&memory_connector.unwrap()).unwrap();
    let unix_client = crate::transport::unix::connect(&unix_addr).await.unwrap();
    let tcp_client = crate::transport::tcp::connect(tcp_addr).await.unwrap();

    let mut memory_client_sub = memory_client.subscribe::<TestPub>("*").await.unwrap();
    let mut unix_client_sub = unix_client.subscribe::<TestPub>("*").await.unwrap();
    let mut tcp_client_sub = tcp_client.subscribe::<TestPub>("*").await.unwrap();

    let memory_client_test_msg = TestPub("m".into());
    memory_client.publish("m", &memory_client_test_msg).await.unwrap();
    assert_eq!(memory_client_test_msg, memory_client_sub.recv().await.unwrap().1);
    assert_eq!(memory_client_test_msg, unix_client_sub.recv().await.unwrap().1);
    assert_eq!(memory_client_test_msg, tcp_client_sub.recv().await.unwrap().1);

    let unix_client_test_msg = TestPub("u".into());
    unix_client.publish("u", &unix_client_test_msg).await.unwrap();
    assert_eq!(unix_client_test_msg, memory_client_sub.recv().await.unwrap().1);
    assert_eq!(unix_client_test_msg, unix_client_sub.recv().await.unwrap().1);
    assert_eq!(unix_client_test_msg, tcp_client_sub.recv().await.unwrap().1);

    let tcp_client_test_msg = TestPub("t".into());
    tcp_client.publish("t", &tcp_client_test_msg).await.unwrap();
    assert_eq!(tcp_client_test_msg, memory_client_sub.recv().await.unwrap().1);
    assert_eq!(tcp_client_test_msg, unix_client_sub.recv().await.unwrap().1);
    assert_eq!(tcp_client_test_msg, tcp_client_sub.recv().await.unwrap().1);

    assert_eq!(Err(BusError::ChannelClosed), stopper.stop().await);
}
