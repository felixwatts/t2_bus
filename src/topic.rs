use crate::err::*;
use regex::Regex;

pub const WILDCARD: &str = "*";
pub const DOUBLE_WILDCARD: &str = "**";

lazy_static! {
    static ref RGX_TOPIC: Regex =
        Regex::new(r"^(([a-z0-9_]+|\*|\*\*)/)*([a-z0-9_]+|\*|\*\*)$").unwrap();
}

#[test]
fn test_rgx_topic() {
    assert!(RGX_TOPIC.is_match("a"));
    assert!(RGX_TOPIC.is_match("abc"));
    assert!(RGX_TOPIC.is_match("0"));
    assert!(RGX_TOPIC.is_match("a0"));
    assert!(RGX_TOPIC.is_match("0a"));
    assert!(RGX_TOPIC.is_match("*"));
    assert!(RGX_TOPIC.is_match("**"));

    assert!(!RGX_TOPIC.is_match("a/"));
    assert!(!RGX_TOPIC.is_match("0/"));
    assert!(!RGX_TOPIC.is_match("*/"));
    assert!(!RGX_TOPIC.is_match("**/"));

    assert!(RGX_TOPIC.is_match("a/b"));
    assert!(RGX_TOPIC.is_match("0/b"));
    assert!(RGX_TOPIC.is_match("a/0"));
    assert!(RGX_TOPIC.is_match("ab/b"));
    assert!(RGX_TOPIC.is_match("a/ba"));
    assert!(RGX_TOPIC.is_match("**/b"));
    assert!(RGX_TOPIC.is_match("*/b"));
    assert!(RGX_TOPIC.is_match("a/*"));
    assert!(RGX_TOPIC.is_match("a/**"));
    assert!(RGX_TOPIC.is_match("**/*"));
    assert!(RGX_TOPIC.is_match("*/**"));
}

pub(crate) fn parse_topic(topic_str: &str) -> BusResult<Vec<&str>> {
    if !RGX_TOPIC.is_match(topic_str) {
        return Err(BusError::InvalidTopicString(format!("Invalid topic string: {}", topic_str)));
    }

    return Ok(topic_str.split('/').collect::<Vec<&str>>());
}

pub fn get_protocol(topic_str: &str) -> BusResult<&str> {
    Ok(parse_topic(topic_str)?[0])
}

pub fn prefix_topic(prefix: &str, topic: &str) -> String {
    match topic {
        "" => prefix.to_string(),
        _ => format!("{}/{}", prefix, topic),
    }
}

pub fn unprefix_topic(topic: &str) -> String {
    let index_of_first_slash = topic.match_indices('/').next().unwrap().0;
    topic[index_of_first_slash + 1..].to_string()
}

#[test]
fn test_unprefix_topic() {
    assert_eq!("a/b/c", &unprefix_topic("prefix/a/b/c"))
}
