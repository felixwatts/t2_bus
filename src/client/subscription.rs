use super::core::{Task, TaskUnsrv, TaskUnsub};
use crate::protocol::{MsgId, MsgUnsrv, MsgUnsub};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

/// Represents a subscription to a protocol `T` and topic. Drop the `Subscription` to unsubscribe.
pub struct Subscription<T> {
    receiver: UnboundedReceiver<T>,
    _subscription_into: SubscriptionInto,
}

impl<T> Subscription<T> {
    pub fn new(
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

/// Represents a subscription to a protocol and topic. Drop the `SubscriptionInto` object to unsubscribe.
pub struct SubscriptionInto {
    topic: String,
    task_sender: UnboundedSender<Task>,
}

impl SubscriptionInto {
    pub(crate) fn new(topic: &str, task_sender: UnboundedSender<Task>) -> SubscriptionInto {
        SubscriptionInto {
            task_sender,
            topic: topic.into(),
        }
    }
}

impl Drop for SubscriptionInto {
    fn drop(&mut self) {
        let task = Task::Unsub(TaskUnsub {
            msg: MsgUnsub {
                topic: self.topic.clone(),
            },
        });

        let _ = self.task_sender.send(task);
    }
}

/// Represents a subscription to requests on a protocol `T` and topic. Returned by `Client::serve`. 
/// While you hold this subscription you are expected to process incoming requests and respond promptly using 
/// `Client::respond`. Drop the `RequestSubscription` object to stop serving.
pub struct RequestSubscription<T> {
    receiver: UnboundedReceiver<(String, MsgId, T)>,
    topic: String,
    task_sender: UnboundedSender<Task>,
}

impl<T> RequestSubscription<T> {
    pub(crate) fn new(
        topic: &str,
        receiver: UnboundedReceiver<(String, MsgId, T)>,
        task_sender: UnboundedSender<Task>,
    ) -> RequestSubscription<T> {
        RequestSubscription {
            receiver,
            task_sender,
            topic: topic.into(),
        }
    }

    /// Receive the next request. Returns `(request_topic, request_id, request_message)`. 
    /// After processing the request you should call `Client::respond` with the `request_id` to send a response
    /// to the requestor. Returns `None` if the feed ends.
    pub async fn recv(&mut self) -> Option<(String, MsgId, T)> {
        self.receiver.recv().await
    }
}

impl<T> Drop for RequestSubscription<T> {
    fn drop(&mut self) {
        let task = Task::Unsrv(TaskUnsrv {
            msg: MsgUnsrv {
                topic: self.topic.clone(),
            },
        });
        let _ = self.task_sender.send(task);
    }
}
