//! Topic and path types.
//!
//! A [`Path`] is a `/` separated sequence of segments with no protocol, e.g. `a/b/c`. The empty string is the
//! root path.
//!
//! A [`Topic`] is a protocol plus a [`Path`]. Its string form is `<protocol>:<path>`, e.g. `f32:a/b/c`.

use crate::err::*;
use regex::Regex;
use serde::{de::Error as _, Deserialize, Deserializer, Serialize, Serializer};
use std::borrow::Cow;
use std::convert::TryFrom;
use std::fmt;
use std::str::FromStr;

pub(crate) const WILDCARD: &str = "*";
pub(crate) const DOUBLE_WILDCARD: &str = "**";

const PROTOCOL_SEPARATOR: char = ':';
const PATH_SEPARATOR: char = '/';

lazy_static! {
    static ref RGX_PATH: Regex =
        Regex::new(r"^(([a-z0-9_]+|\*|\*\*)/)*([a-z0-9_]+|\*|\*\*)$").unwrap();
    static ref RGX_PROTOCOL: Regex = Regex::new(r"^([a-z0-9_]+|\*)$").unwrap();
}

/// A `/` separated path without a protocol, e.g. `a/b/c`. The empty path is the root.
///
/// Segments consist of lowercase letters, digits and underscores, or are one of the wildcards `*` (matches exactly
/// one segment) or `**` (matches zero or more segments).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Path(String);

impl Path {
    /// The root (empty) path.
    pub fn root() -> Path {
        Path(String::new())
    }

    /// Validate and construct a path.
    pub fn new(path: impl Into<String>) -> BusResult<Path> {
        let path = path.into();
        validate_path(&path)?;
        Ok(Path(path))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_root(&self) -> bool {
        self.0.is_empty()
    }

    /// The segments of this path. Yields nothing for the root path.
    pub fn segments(&self) -> impl Iterator<Item = &str> + Clone {
        let path = if self.is_root() { None } else { Some(&self.0) };
        path.into_iter().flat_map(|p| p.split(PATH_SEPARATOR))
    }

    /// Construct from segments already known to be valid.
    fn from_valid_segments<'a>(segments: impl Iterator<Item = &'a str>) -> Path {
        let mut path = String::new();
        for (i, segment) in segments.enumerate() {
            if i > 0 {
                path.push(PATH_SEPARATOR);
            }
            path.push_str(segment);
        }
        Path(path)
    }
}

fn validate_path(path: &str) -> BusResult<()> {
    if path.is_empty() || RGX_PATH.is_match(path) {
        Ok(())
    } else {
        Err(BusError::InvalidTopicString(format!(
            "Invalid path: {path}"
        )))
    }
}

fn validate_protocol(protocol: &str) -> BusResult<()> {
    if RGX_PROTOCOL.is_match(protocol) {
        Ok(())
    } else {
        Err(BusError::InvalidTopicString(format!(
            "Invalid protocol: {protocol}"
        )))
    }
}

impl FromStr for Path {
    type Err = BusError;

    fn from_str(s: &str) -> BusResult<Path> {
        Path::new(s)
    }
}

impl TryFrom<&str> for Path {
    type Error = BusError;

    fn try_from(s: &str) -> BusResult<Path> {
        Path::new(s)
    }
}

impl TryFrom<String> for Path {
    type Error = BusError;

    fn try_from(s: String) -> BusResult<Path> {
        Path::new(s)
    }
}

impl AsRef<str> for Path {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for Path {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Path {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Path, D::Error> {
        let s = String::deserialize(deserializer)?;
        Path::new(s).map_err(D::Error::custom)
    }
}

/// A protocol plus a [`Path`]. The string form is `<protocol>:<path>`, e.g. `f32:a/b/c`, or `f32:` for the root
/// path. The protocol may be the wildcard `*` to match topics of any protocol.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Topic {
    protocol: Cow<'static, str>,
    path: Path,
}

impl Topic {
    /// Validate and construct a topic. A `&'static str` protocol (such as `PublishProtocol::prefix()`) is
    /// borrowed rather than copied.
    pub fn new(protocol: impl Into<Cow<'static, str>>, path: Path) -> BusResult<Topic> {
        let protocol = protocol.into();
        validate_protocol(&protocol)?;
        Ok(Topic { protocol, path })
    }

    pub fn protocol(&self) -> &str {
        &self.protocol
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Split into protocol and path without copying.
    pub fn into_parts(self) -> (Cow<'static, str>, Path) {
        (self.protocol, self.path)
    }

    /// All segments of the topic: the protocol followed by the path segments.
    pub(crate) fn segments(&self) -> impl Iterator<Item = &str> + Clone {
        std::iter::once(self.protocol.as_ref()).chain(self.path.segments())
    }

    /// Construct from segments (protocol first) already known to be valid. Returns `None` if there are no segments.
    pub(crate) fn from_valid_segments(segments: &[&str]) -> Option<Topic> {
        let (protocol, path) = segments.split_first()?;
        Some(Topic {
            protocol: Cow::Owned((*protocol).to_string()),
            path: Path::from_valid_segments(path.iter().copied()),
        })
    }
}

impl FromStr for Topic {
    type Err = BusError;

    fn from_str(s: &str) -> BusResult<Topic> {
        let (protocol, path) = s.split_once(PROTOCOL_SEPARATOR).ok_or_else(|| {
            BusError::InvalidTopicString(format!("Invalid topic: {s}. Expected <protocol>:<path>"))
        })?;
        Topic::new(protocol.to_string(), path.parse()?)
    }
}

impl TryFrom<&str> for Topic {
    type Error = BusError;

    fn try_from(s: &str) -> BusResult<Topic> {
        s.parse()
    }
}

impl fmt::Display for Topic {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}{PROTOCOL_SEPARATOR}{}", self.protocol, self.path)
    }
}

// On the wire a topic is a `(protocol, path)` pair, which avoids building the joined string on every send.
impl Serialize for Topic {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        (self.protocol.as_ref(), &self.path).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Topic {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Topic, D::Error> {
        let (protocol, path) = <(String, Path)>::deserialize(deserializer)?;
        Topic::new(protocol, path).map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rgx_path() {
        for ok in [
            "a", "abc", "0", "a0", "0a", "*", "**", "a/b", "0/b", "a/0", "ab/b", "a/ba", "**/b",
            "*/b", "a/*", "a/**", "**/*", "*/**", "1/2", "a_1/2_a_", "1_/_2",
        ] {
            assert!(RGX_PATH.is_match(ok), "{}", ok);
        }
        for bad in ["a/", "0/", "*/", "**/", "/a", "a//b", "A", "a:b", "***"] {
            assert!(!RGX_PATH.is_match(bad), "{}", bad);
        }
    }

    #[test]
    fn test_path() {
        assert!(Path::new("").unwrap().is_root());
        assert_eq!(Path::root(), "".parse().unwrap());
        assert_eq!(0, Path::root().segments().count());
        assert_eq!(
            vec!["a", "*", "**"],
            Path::new("a/*/**").unwrap().segments().collect::<Vec<_>>()
        );
        assert!(Path::new("a/").is_err());
        assert!(Path::new("f32:a").is_err());
        assert_eq!("a/b", Path::new("a/b").unwrap().to_string());
    }

    #[test]
    fn test_topic_parse_and_display() {
        for s in ["f32:a/b/c", "f32:", "*:**", "a_1:*"] {
            assert_eq!(s, s.parse::<Topic>().unwrap().to_string());
        }

        let topic: Topic = "f32:a/b".parse().unwrap();
        assert_eq!("f32", topic.protocol());
        assert_eq!("a/b", topic.path().as_str());
        assert_eq!(vec!["f32", "a", "b"], topic.segments().collect::<Vec<_>>());
        assert_eq!(
            vec!["f32"],
            "f32:"
                .parse::<Topic>()
                .unwrap()
                .segments()
                .collect::<Vec<_>>()
        );

        for bad in ["f32", "f32/a", ":a", "**:a", "F32:a", "f32:a/", "f32:a:b"] {
            assert!(bad.parse::<Topic>().is_err(), "{}", bad);
        }
    }

    #[test]
    fn test_topic_new() {
        let topic = Topic::new("f32", Path::new("a").unwrap()).unwrap();
        assert!(matches!(topic.protocol, Cow::Borrowed(_)));
        assert!(Topic::new("f/32", Path::root()).is_err());
        assert!(Topic::new("", Path::root()).is_err());
    }

    #[test]
    fn test_topic_from_valid_segments() {
        assert_eq!(None, Topic::from_valid_segments(&[]));
        assert_eq!(
            "p:".parse::<Topic>().unwrap(),
            Topic::from_valid_segments(&["p"]).unwrap()
        );
        assert_eq!(
            "p:a/b".parse::<Topic>().unwrap(),
            Topic::from_valid_segments(&["p", "a", "b"]).unwrap()
        );
    }

    #[test]
    fn test_serde_round_trip() {
        let topic: Topic = "f32:a/*/**".parse().unwrap();
        let bytes = crate::transport::cbor_codec::ser(&topic).unwrap();
        let decoded: Topic = crate::transport::cbor_codec::deser(&bytes).unwrap();
        assert_eq!(topic, decoded);

        let bad = crate::transport::cbor_codec::ser(&("f32", "a/")).unwrap();
        assert!(crate::transport::cbor_codec::deser::<Topic>(&bad).is_err());
    }
}
