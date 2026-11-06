use crate::err::*;
use crate::topic::*;
use std::collections::{HashMap, HashSet};

/// The `Directory` is responsible for topic matching. It stores the `id`s of subscribed and serving clients
/// in a data structure that allows matching subscribers or server to be looked for for a given topic.
pub struct Directory {
    owner: Option<u32>,
    children: HashMap<String, Directory>,
    subscribers: HashSet<u32>,
}

impl Directory {
    pub(crate) fn new() -> Directory {
        Directory {
            owner: None,
            children: HashMap::new(),
            subscribers: HashSet::new(),
        }
    }

    /// Claim the specified topic. The given `client` will become the owner of the given `topic`. Errors if the topic
    /// is already claimed
    pub(crate) fn claim(&mut self, client: u32, topic: &Topic) -> BusResult<()> {
        self.claim_segments(client, &segments(topic))
    }

    /// Unclaim the given topic. After this the given `client` will not be the owner of the given `topic`
    pub(crate) fn unclaim(&mut self, client: u32, topic: &Topic) -> BusResult<()> {
        self.unclaim_segments(client, &segments(topic))
    }

    /// Gets the client that is the owner of the given `topic`.
    pub(crate) fn get_owner(&self, topic: &Topic) -> Option<u32> {
        self.get_owner_segments(&segments(topic))
    }

    /// Subscribe the given `client` to the given `topic`. The client will then be returned by `get_subscribers`
    /// for matching topics
    pub(crate) fn subscribe(&mut self, client: u32, topic: &Topic) {
        self.subscribe_segments(client, &segments(topic))
    }

    /// Unsubscribe the given `client` from the given `topic`. The client will no longer be returned by `get_subscribers`
    /// for matching topics
    pub(crate) fn unsubscribe(&mut self, client: u32, topic: &Topic) -> BusResult<()> {
        self.unsubscribe_segments(client, &segments(topic))
    }

    /// Claim the specified topic. The given `client` will become the owner of the given `topic`. Errors if the topic
    /// is already claimed
    fn claim_segments(&mut self, client: u32, topic: &[&str]) -> BusResult<()> {
        if topic.is_empty() {
            if let Some(owner) = self.owner {
                return if owner == client {
                    return Ok(());
                } else {
                    Err(BusError::TopicAlreadyClaimed)
                };
            }

            self.owner = Some(client);
            return Ok(());
        }

        if topic[0] == WILDCARD || topic[0] == DOUBLE_WILDCARD {
            return Err(BusError::ClaimWildcardNotSupported);
        }

        self.ensure_child(topic[0]);

        self.children
            .get_mut(topic[0])
            .unwrap()
            .claim_segments(client, &topic[1..])
    }

    /// Unclaim the given topic. After this the given `client` will not be the owner of the given `topic`
    fn unclaim_segments(&mut self, client: u32, topic: &[&str]) -> BusResult<()> {
        if topic.is_empty() {
            if let Some(owner) = self.owner {
                if owner == client {
                    self.owner = None;
                }
            }

            return Ok(());
        }

        if topic[0] == WILDCARD || topic[0] == DOUBLE_WILDCARD {
            return Err(BusError::ClaimWildcardNotSupported);
        }

        let child = self.children.get_mut(topic[0]);

        match child {
            Some(child) => child.unclaim_segments(client, &topic[1..]),
            None => Ok(()),
        }
    }

    /// Gets the client that is the owner of the given `topic`.
    fn get_owner_segments(&self, topic: &[&str]) -> Option<u32> {
        if topic.is_empty() {
            return self.owner;
        }

        if topic[0] == WILDCARD || topic[0] == DOUBLE_WILDCARD {
            return None; // TODO return result?
        }

        if let Some(child) = self.children.get(topic[0]) {
            return child.get_owner_segments(&topic[1..]);
        }

        None
    }

    /// Subscribe the given `client` to the given `topic`. The client will then be returned by `get_subscribers`
    /// for matching topics
    fn subscribe_segments(&mut self, client: u32, topic: &[&str]) {
        if topic.is_empty() {
            self.subscribers.insert(client);
            return;
        }

        self.ensure_child(topic[0]);

        self.children
            .get_mut(topic[0])
            .expect("internal error")
            .subscribe_segments(client, &topic[1..])
    }

    /// Unsubscribe the given `client` from the given `topic`. The client will no longer be returned by `get_subscribers`
    /// for matching topics
    fn unsubscribe_segments(&mut self, client: u32, topic: &[&str]) -> BusResult<()> {
        if topic.is_empty() {
            self.subscribers.remove(&client);
            return Ok(());
        }

        if let Some(child) = self.children.get_mut(topic[0]) {
            return child.unsubscribe_segments(client, &topic[1..]);
        }

        Ok(())
    }

    /// Gets any clients that are currently subscribed for topics matching the given `topic`
    pub(crate) fn get_subscribers(&self, topic: &Topic) -> Vec<u32> {
        let mut result = HashSet::new();
        self._get_subscribers(&segments(topic), &mut result, false);
        result.drain().collect()
    }

    // Unsubscribes `client` from all topics and returns a list of topics
    // that newly have no subscribers
    pub(crate) fn drop_client(&mut self, client: u32) -> Vec<Topic> {
        let mut topic = vec![];
        let mut results = vec![];
        self._drop_client(client, &mut topic, &mut results);
        results
    }

    fn _drop_client<'a>(
        &'a mut self,
        client: u32,
        topic: &mut Vec<&'a str>,
        results: &mut Vec<Topic>,
    ) {
        if let Some(owner) = self.owner {
            if owner == client {
                self.owner = None
            }
        }

        if self.subscribers.remove(&client) && self.subscribers.is_empty() {
            // Every topic has at least a protocol segment, so the root node never has subscribers
            if let Some(topic) = Topic::from_valid_segments(topic) {
                results.push(topic);
            }
        }

        for (segment, child) in self.children.iter_mut() {
            topic.push(segment);
            child._drop_client(client, topic, results);
            topic.pop();
        }
    }

    /// Collect the subscribers of this node's subtree whose subscriptions match `topic`, the remaining
    /// segments of the published topic.
    ///
    /// `in_double_wildcard` is true when this node is a subscribed `**` that may still consume segments. A
    /// subscribed `**` matches zero or more segments, so it may stop here or consume the next segment.
    fn _get_subscribers(
        &self,
        topic: &[&str],
        subscribers: &mut HashSet<u32>,
        in_double_wildcard: bool,
    ) {
        if in_double_wildcard {
            self._get_subscribers(topic, subscribers, false);
            if !topic.is_empty() {
                self._get_subscribers(&topic[1..], subscribers, true);
            }
            return;
        }

        if topic.is_empty() {
            subscribers.extend(self.subscribers.iter());
            // A subscribed `**` can match the empty remainder
            if let Some(child) = self.children.get(DOUBLE_WILDCARD) {
                child._get_subscribers(topic, subscribers, true);
            }
            return;
        }

        // A published `**` matches zero or more of the subscribed segments
        if topic[0] == DOUBLE_WILDCARD {
            self._get_subscribers(&topic[1..], subscribers, false);
            for child in self.children.values() {
                child._get_subscribers(topic, subscribers, false);
            }
            return;
        }

        // A published `*` matches any one subscribed segment
        if topic[0] == WILDCARD {
            for (segment, child) in self.children.iter() {
                if segment == DOUBLE_WILDCARD {
                    child._get_subscribers(topic, subscribers, true);
                } else {
                    child._get_subscribers(&topic[1..], subscribers, false);
                }
            }
            return;
        }

        if let Some(child) = self.children.get(topic[0]) {
            child._get_subscribers(&topic[1..], subscribers, false);
        }

        if let Some(child) = self.children.get(WILDCARD) {
            child._get_subscribers(&topic[1..], subscribers, false);
        }

        if let Some(child) = self.children.get(DOUBLE_WILDCARD) {
            child._get_subscribers(topic, subscribers, true);
        }
    }

    fn ensure_child(&mut self, name: &str) {
        if !self.children.contains_key(name) {
            self.children.insert(name.to_string(), Directory::new());
        }
    }
}

fn segments(topic: &Topic) -> Vec<&str> {
    topic.segments().collect()
}

#[cfg(test)]
fn test_topic(path: &str) -> Topic {
    Topic::new("p", path.parse().unwrap()).unwrap()
}

#[test]
fn test_drop_client() {
    _test_drop_client(vec![vec![]]);
    _test_drop_client(vec![vec!["a"]]);
    _test_drop_client(vec![vec!["a/b"]]);
    _test_drop_client(vec![vec!["a", "b"]]);
    _test_drop_client(vec![vec!["a"], vec![]]);
    _test_drop_client(vec![vec!["a"], vec!["a"]]);
    _test_drop_client(vec![vec!["a/b"], vec!["a"]]);
    _test_drop_client(vec![vec!["a"], vec!["a/b"]]);
    _test_drop_client(vec![vec!["a/b"], vec!["a/b"]]);
    _test_drop_client(vec![vec!["a"], vec!["b"]]);
    _test_drop_client(vec![vec!["a", "b"], vec!["a"]]);
    _test_drop_client(vec![vec!["a"], vec!["a", "b"]]);
    _test_drop_client(vec![vec!["*"], vec![]]);
    _test_drop_client(vec![vec!["*"], vec!["a"]]);
    _test_drop_client(vec![vec!["*"], vec!["*"]]);
    _test_drop_client(vec![vec!["*", "a"], vec![]]);
    _test_drop_client(vec![vec!["*", "a"], vec!["a"]]);
    _test_drop_client(vec![vec!["*", "a"], vec!["*"]]);
    _test_drop_client(vec![vec!["**"], vec![]]);
    _test_drop_client(vec![vec!["**"], vec!["a"]]);
    _test_drop_client(vec![vec!["**"], vec!["*"]]);
    _test_drop_client(vec![vec!["**", "a"], vec![]]);
    _test_drop_client(vec![vec!["**", "a"], vec!["a"]]);
    _test_drop_client(vec![vec!["**", "a"], vec!["*"]]);
}

#[cfg(test)]
fn _test_drop_client(subscriptions: Vec<Vec<&str>>) {
    let mut subject = Directory::new();
    for (i, sub) in subscriptions.iter().enumerate() {
        for &topic in sub {
            subject.subscribe(i as u32, &test_topic(topic));
        }
    }

    let empty_topics = subject.drop_client(0);
    // TODO assert_eq!(subscriptions[0].len(), topics.len());
    for topic in &empty_topics {
        assert!(subscriptions[0].contains(&topic.path().as_str()));
    }

    for topic in &subscriptions[0] {
        let subscribers = subject.get_subscribers(&test_topic(topic));
        assert!(!subscribers.contains(&0));
    }
}

#[test]
fn test_drop_client_claim() {
    _test_drop_client_claim("a");
    _test_drop_client_claim("a/b");
}

#[cfg(test)]
fn _test_drop_client_claim(topic_str: &str) {
    let mut subject = Directory::new();
    let topic = test_topic(topic_str);

    subject.claim(0, &topic).unwrap();
    subject.drop_client(0);

    assert!(subject.get_owner(&topic).is_none());
}

#[test]
fn test_ownership() {
    _test_ownership_happy_path("a");
    _test_ownership_happy_path("a/b");
}

#[cfg(test)]
fn _test_ownership_happy_path(topic_str: &str) {
    let mut subject = Directory::new();
    let topic = test_topic(topic_str);
    subject.claim(0, &topic).unwrap();
    assert_eq!(0, subject.get_owner(&topic).unwrap());
}

#[test]
fn test_ownership_no_duplicate() {
    let mut subject = Directory::new();
    let topic = test_topic("a");
    subject.claim(0, &topic).unwrap();
    subject.claim(1, &topic).unwrap_err();
    assert_eq!(0, subject.get_owner(&topic).unwrap());
}

#[test]
fn test_ownership_no_wildcard() {
    _test_ownership_no_wildcard("*");
    _test_ownership_no_wildcard("**");
    _test_ownership_no_wildcard("*/a");
    _test_ownership_no_wildcard("a/*");
    _test_ownership_no_wildcard("**/a");
    _test_ownership_no_wildcard("a/**");
}

#[cfg(test)]
fn _test_ownership_no_wildcard(topic_str: &str) {
    let mut subject = Directory::new();
    let topic = test_topic(topic_str);
    subject.claim(0, &topic).unwrap_err();
    assert!(subject.get_owner(&topic).is_none());
}

#[test]
fn test_ownership_unclaim() {
    let mut subject = Directory::new();
    let topic = test_topic("a");
    subject.claim(0, &topic).unwrap();
    subject.unclaim(0, &topic).unwrap();
    assert!(subject.get_owner(&topic).is_none());
    subject.claim(1, &topic).unwrap();
}

#[test]
fn test_topic_matching() {
    assert_keys_match("a", "a", true);
    assert_keys_match("a", "a", true);
    assert_keys_match("a", "b", false);
    assert_keys_match("a/b", "a/b", true);
    assert_keys_match("a/*", "a/b", true);
    assert_keys_match("*/b", "a/b", true);
    assert_keys_match("*/c", "a/b", false);
    assert_keys_match("c/b", "a/b", false);
    assert_keys_match("a/**", "a/b/c", true);
    assert_keys_match("a/**/d", "a/b/c/d", true);
    assert_keys_match("a/**/d", "a/b/c/d/e", false);
    assert_keys_match("a/**/d/*", "a/b/c/d/e", true);
    assert_keys_match("a/**/d/f", "a/b/c/d/e", false);
    assert_keys_match("a", "*", true);
    assert_keys_match("a/b", "*", false);
    assert_keys_match("a/b", "a/*", true);
    assert_keys_match("a/b", "*/b", true);
    assert_keys_match("a/b", "*/c", false);
    assert_keys_match("a/b", "*/*", true);
    assert_keys_match("*", "*", true);
    assert_keys_match("*/b", "*/b", true);
    assert_keys_match("a/*", "*/b", true);
    assert_keys_match("*/b", "a/*", true);
    assert_keys_match("*/*", "*/*", true);
    assert_keys_match("a", "**", true);
    assert_keys_match("a/b/c", "a/**", true);
    assert_keys_match("a/b/c", "a/**/c", true);
    assert_keys_match("a/b/c", "a/**/d", false);
    assert_keys_match("a/b/c/d", "a/**/d", true);
    assert_keys_match("a/*/d", "a/**/d", true);
    assert_keys_match("a/**/d", "a/**/c/d", true);
    assert_keys_match("a/**/c/d", "a/**/d", true);
    assert_keys_match("a/**/c/d", "a/**/c/d", true);
    assert_keys_match("a/b/**/d", "a/**/c/d", true);

    // `**` matches zero segments
    assert_keys_match("a/**", "a", true);
    assert_keys_match("a", "a/**", true);
    assert_keys_match("a/**/b", "a/b", true);
    assert_keys_match("a/b", "a/**/b", true);
    assert_keys_match("**/a", "a", true);
    assert_keys_match("a/**/**/b", "a/b", true);
    assert_keys_match("a/**/*", "a/b", true);
    assert_keys_match("a/**/*", "a", false);
    assert_keys_match("a/**/b", "a/c", false);
    assert_keys_match("a/**", "b", false);
    assert_keys_match("a/**/b", "a", false);

    // a subscribed `**` against a published `*`
    assert_keys_match("a/**", "a/*/b", true);
    assert_keys_match("a/**/b", "a/*", true);
    assert_keys_match("a/**/c", "a/*/b", false);
}

#[cfg(test)]
fn assert_keys_match(topic_sub_str: &str, topic_pub_str: &str, expect: bool) {
    let mut subject = Directory::new();
    let topic_sub = test_topic(topic_sub_str);
    let topic_pub = test_topic(topic_pub_str);

    subject.subscribe(0, &topic_sub);
    let subscribers = subject.get_subscribers(&topic_pub);

    if expect {
        assert_eq!(1, subscribers.len());
    } else {
        assert_eq!(0, subscribers.len());
    }
}

#[test]
fn test_protocol_matching() {
    let assert_match = |sub: &str, publ: &str, expect: bool| {
        let mut subject = Directory::new();
        subject.subscribe(0, &sub.parse().unwrap());
        let n = subject.get_subscribers(&publ.parse().unwrap()).len();
        assert_eq!(expect as usize, n, "{sub} vs {publ}");
    };
    assert_match("p:a", "p:a", true);
    assert_match("p:a", "q:a", false);
    assert_match("p:", "p:", true);
    assert_match("p:", "p:a", false);
    assert_match("*:a", "p:a", true);
    assert_match("*:a", "q:a", true);
    assert_match("*:**", "p:a/b", true);
    assert_match("p:**", "q:a", false);
    // `**` matches zero segments, so it matches the root path
    assert_match("*:**", "p:", true);
    assert_match("p:**", "p:", true);
    assert_match("p:**/**", "p:", true);
    assert_match("p:", "p:**", true);
    assert_match("p:a", "p:", false);
}

#[test]
fn test_drop_client_returns_topics() {
    let mut subject = Directory::new();
    let topic: Topic = "p:a/b".parse().unwrap();
    subject.subscribe(0, &topic);
    subject.subscribe(0, &"q:".parse().unwrap());
    let mut dropped = subject.drop_client(0);
    dropped.sort_by_key(|t| t.to_string());
    assert_eq!(vec![topic, "q:".parse().unwrap()], dropped);
}
