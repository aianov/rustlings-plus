use std::fmt;
use std::str::FromStr;

use crate::command::ParseError;
use crate::ids::{PostId, digits};

pub type Timestamp = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Cursor {
    pub time: Timestamp,
    pub post: PostId,
}

impl fmt::Display for Cursor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.time, self.post.0)
    }
}

impl FromStr for Cursor {
    type Err = ParseError;

    fn from_str(text: &str) -> Result<Cursor, ParseError> {
        let parts = text.split_once(':');
        let Some((time, post)) =
            parts.and_then(|(time, post)| Some((digits(time)?, digits(post)?)))
        else {
            return Err(ParseError::BadCursor(text.to_string()));
        };
        Ok(Cursor {
            time,
            post: PostId(post),
        })
    }
}
