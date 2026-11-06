use super::core::{Task, TaskUnsrv, TaskUnsub};
use crate::protocol::{MsgId, UnsrvMsg, UnsubMsg};
use crate::topic::{Path, Topic};
use futures::Stream;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

/// Represents a subscription to a protocol `T` and topic. Drop the `Subscription` to unsubscribe.
pub struct Subscription<T> {
    receiver: UnboundedReceiver<T>,
    _subscription_into: SubscriptionInto,
}

impl<T> Subscription<T> {
    pub(crate) fn new(
        receiver: UnboundedReceiver<T>,
        subscription_into: SubscriptionInto,
    ) -> Subscription<T> {
        Subscription {
            receiver,
            _subscription_into: subscription_into,
        }
    }

    /// Receive the next message in the feed. Returns `None` if the feed ends.
    pub async fn recv(&mut self) -> Option<T> {
        self.receiver.recv().await
    }
}

impl<T> Stream for Subscription<T> {
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.get_mut().receiver.poll_recv(cx)
    }
}

/// Represents a subscription to a protocol and topic, produced by `Client::subscribe_bytes_into`. Drop the `SubscriptionInto` object to unsubscribe.
pub struct SubscriptionInto {
    // Always `Some` until dropped. `Option` lets `drop` move the topic out rather than clone it.
    topic: Option<Topic>,
    task_sender: UnboundedSender<Task>,
}

impl SubscriptionInto {
    pub(crate) fn new(topic: Topic, task_sender: UnboundedSender<Task>) -> SubscriptionInto {
        SubscriptionInto {
            task_sender,
            topic: Some(topic),
        }
    }
}

impl Drop for SubscriptionInto {
    fn drop(&mut self) {
        if let Some(topic) = self.topic.take() {
            let task = Task::Unsub(TaskUnsub {
                msg: UnsubMsg { topic },
            });

            let _ = self.task_sender.send(task);
        }
    }
}

/// Represents a subscription to requests on a protocol `T` and topic. Returned by `Client::serve`.
///
/// While you hold this subscription you are expected to process incoming requests and respond promptly using
/// `Client::respond`. Drop the `RequestSubscription` object to stop serving.
pub struct RequestSubscription<T> {
    receiver: UnboundedReceiver<(Path, MsgId, T)>,
    // Always `Some` until dropped. `Option` lets `drop` move the topic out rather than clone it.
    topic: Option<Topic>,
    task_sender: UnboundedSender<Task>,
}

impl<T> RequestSubscription<T> {
    pub(crate) fn new(
        topic: Topic,
        receiver: UnboundedReceiver<(Path, MsgId, T)>,
        task_sender: UnboundedSender<Task>,
    ) -> RequestSubscription<T> {
        RequestSubscription {
            receiver,
            task_sender,
            topic: Some(topic),
        }
    }

    /// Receive the next request. Returns `(request_path, request_id, request_message)`.
    /// After processing the request you should call `Client::respond` with the `request_id` to send a response
    /// to the requestor. Returns `None` if the feed ends.
    pub async fn recv(&mut self) -> Option<(Path, MsgId, T)> {
        self.receiver.recv().await
    }
}

impl<T> Stream for RequestSubscription<T> {
    type Item = (Path, MsgId, T);

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.get_mut().receiver.poll_recv(cx)
    }
}

impl<T> Drop for RequestSubscription<T> {
    fn drop(&mut self) {
        if let Some(topic) = self.topic.take() {
            let task = Task::Unsrv(TaskUnsrv {
                msg: UnsrvMsg { topic },
            });
            let _ = self.task_sender.send(task);
        }
    }
}
