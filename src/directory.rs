use crate::topic::*;
use crate::err::*;
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
    pub(crate) fn claim(&mut self, client: u32, topic: &[&str]) -> BusResult<()> {
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
            .claim(client, &topic[1..])
    }

    /// Unclaim the given topic. After this the given `client` will not be the owner of the given `topic`
    pub(crate) fn unclaim(&mut self, client: u32, topic: &[&str]) -> BusResult<()> {
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
            Some(child) => child.unclaim(client, &topic[1..]),
            None => Ok(()),
        }
    }

    /// Gets the client that is the owner of the given `topic`.
    pub(crate) fn get_owner(&self, topic: &[&str]) -> Option<u32> {
        if topic.is_empty() {
            return self.owner;
        }

        if topic[0] == WILDCARD || topic[0] == DOUBLE_WILDCARD {
            return None; // TODO return result?
        }

        if let Some(child) = self.children.get(topic[0]) {
            return child.get_owner(&topic[1..]);
        }

        None
    }

    /// Subscribe the given `client` to the given `topic`. The client will then be returned by `get_subscribers`
    /// for matching topics
    pub(crate) fn subscribe(&mut self, client: u32, topic: &[&str]) {
        if topic.is_empty() {
            self.subscribers.insert(client);
            return;
        }

        self.ensure_child(topic[0]);

        self.children
            .get_mut(topic[0])
            .expect("internal error")
            .subscribe(client, &topic[1..])
    }

    /// Unsubscribe the given `client` from the given `topic`. The client will no longer be returned by `get_subscribers`
    /// for matching topics
    pub(crate) fn unsubscribe(&mut self, client: u32, topic: &[&str]) -> BusResult<()> {
        if topic.is_empty() {
            self.subscribers.remove(&client);
            return Ok(());
        }

        if let Some(child) = self.children.get_mut(topic[0]) {
            return child.unsubscribe(client, &topic[1..]);
        }

        Ok(())
    }

    /// Gets any clients that are currently subscribed for topics matching the given `topic`
    pub(crate) fn get_subscribers(&self, topic: &[&str]) -> Vec<u32> {
        let mut result = HashSet::new();
        self._get_subscribers(topic, &mut result, false);
        result.drain().collect()
    }

    // Unsubscribes `client` from all topics and returns a list of topics
    // that newly have no subscribers
    pub(crate) fn drop_client(&mut self, client: u32) -> Vec<String> {
        let mut topic = vec![];
        let mut results = vec![];
        self._drop_client(client, &mut topic, &mut results);
        results
    }

    fn _drop_client(&mut self, client: u32, topic: &mut Vec<String>, results: &mut Vec<String>) {
        if let Some(owner) = self.owner {
            if owner == client {
                self.owner = None
            }
        }

        if self.subscribers.remove(&client) && self.subscribers.is_empty() {
            let topic_str = topic.join("/");
            results.push(topic_str);
        }

        for kvp in self.children.iter_mut() {
            topic.push(kvp.0.to_string());
            kvp.1._drop_client(client, topic, results);
            topic.pop();
        }
    }

    fn _get_subscribers(
        &self,
        topic: &[&str],
        subscribers: &mut HashSet<u32>,
        double_wildcard: bool,
    ) {
        if topic.is_empty() {
            subscribers.extend(self.subscribers.iter());
            return;
        }

        if topic[0] == DOUBLE_WILDCARD {
            self._get_subscribers(&topic[1..], subscribers, double_wildcard);

            for child in self.children.iter() {
                child
                    .1
                    ._get_subscribers(topic, subscribers, double_wildcard);
            }
        }

        if double_wildcard {
            self._get_subscribers(&topic[1..], subscribers, true);

            if let Some(child) = self.children.get(topic[0]) {
                child._get_subscribers(&topic[1..], subscribers, false);
            }

            return;
        }

        if topic[0] == WILDCARD {
            for child in self.children.iter() {
                child
                    .1
                    ._get_subscribers(&topic[1..], subscribers, double_wildcard);
            }

            return;
        }

        if let Some(child) = self.children.get(topic[0]) {
            child._get_subscribers(&topic[1..], subscribers, double_wildcard);
        }

        if let Some(child) = self.children.get(WILDCARD) {
            child._get_subscribers(&topic[1..], subscribers, double_wildcard);
        }

        if let Some(child) = self.children.get(DOUBLE_WILDCARD) {
            child._get_subscribers(&topic[1..], subscribers, true);
        }
    }

    fn ensure_child(&mut self, name: &str) {
        if !self.children.contains_key(name) {
            self.children.insert(name.to_string(), Directory::new());
        }
    }
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

fn _test_drop_client(subscriptions: Vec<Vec<&str>>) {
    let mut subject = Directory::new();
    for (i, sub) in subscriptions.iter().enumerate() {
        for &topic in sub {
            subject.subscribe(i as u32, &parse_topic(topic).unwrap());
        }
    }

    let empty_topics = subject.drop_client(0);
    // TODO assert_eq!(subscriptions[0].len(), topics.len());
    for topic in &empty_topics {
        assert!(subscriptions[0].contains(&topic.as_str()));
    }

    for topic in &subscriptions[0] {
        let subscribers = subject.get_subscribers(&parse_topic(topic).unwrap());
        assert!(!subscribers.contains(&0));
    }
}

#[test]
fn test_drop_client_claim() {
    _test_drop_client_claim("a");
    _test_drop_client_claim("a/b");
}

fn _test_drop_client_claim(topic_str: &str) {
    let mut subject = Directory::new();
    let topic = parse_topic(topic_str).unwrap();

    subject.claim(0, &topic).unwrap();
    subject.drop_client(0);

    assert!(subject.get_owner(&topic).is_none());
}

#[test]
fn test_ownership() {
    _test_ownership_happy_path("a");
    _test_ownership_happy_path("a/b");
}

fn _test_ownership_happy_path(topic_str: &str) {
    let mut subject = Directory::new();
    let topic = parse_topic(topic_str).unwrap();
    subject.claim(0, &topic).unwrap();
    assert_eq!(0, subject.get_owner(&topic).unwrap());
}

#[test]
fn test_ownership_no_duplicate() {
    let mut subject = Directory::new();
    let topic = parse_topic("a").unwrap();
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

fn _test_ownership_no_wildcard(topic_str: &str) {
    let mut subject = Directory::new();
    let topic = parse_topic(topic_str).unwrap();
    subject.claim(0, &topic).unwrap_err();    
    assert!(subject.get_owner(&topic).is_none());
}

#[test]
fn test_ownership_unclaim() {
    let mut subject = Directory::new();
    let topic = parse_topic("a").unwrap();
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
}

#[cfg(test)]
fn assert_keys_match(topic_sub_str: &str, topic_pub_str: &str, expect: bool) {
    let mut subject = Directory::new();
    let topic_sub = parse_topic(topic_sub_str).unwrap();
    let topic_pub = parse_topic(topic_pub_str).unwrap();

    subject.subscribe(0, &topic_sub);
    let subscribers = subject.get_subscribers(&topic_pub);

    if expect {
        assert_eq!(1, subscribers.len());
    } else {
        assert_eq!(0, subscribers.len());
    }
}
