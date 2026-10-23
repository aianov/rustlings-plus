use std::error::Error;
use std::fmt;
use std::str::FromStr;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

const USER_PREFIX: &str = "u_";
const POST_PREFIX: &str = "p_";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UserId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PostId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdError {
    prefix: &'static str,
    text: String,
}

impl fmt::Display for IdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "неверный id {:?}: нужно {}<число>",
            self.text, self.prefix
        )
    }
}

impl Error for IdError {}

fn parse_id(prefix: &'static str, text: &str) -> Result<u64, IdError> {
    text.strip_prefix(prefix)
        .filter(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|digits| digits.parse().ok())
        .ok_or_else(|| IdError {
            prefix,
            text: text.to_string(),
        })
}

impl fmt::Display for UserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{USER_PREFIX}{}", self.0)
    }
}

impl FromStr for UserId {
    type Err = IdError;

    fn from_str(text: &str) -> Result<UserId, IdError> {
        parse_id(USER_PREFIX, text).map(UserId)
    }
}

impl Serialize for UserId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for UserId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<UserId, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(D::Error::custom)
    }
}

impl fmt::Display for PostId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{POST_PREFIX}{}", self.0)
    }
}

impl FromStr for PostId {
    type Err = IdError;

    fn from_str(text: &str) -> Result<PostId, IdError> {
        parse_id(POST_PREFIX, text).map(PostId)
    }
}

impl Serialize for PostId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for PostId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<PostId, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(D::Error::custom)
    }
}
