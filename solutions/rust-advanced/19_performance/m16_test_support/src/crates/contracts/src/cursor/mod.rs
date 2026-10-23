use std::fmt;
use std::str::FromStr;

use crate::command::ParseError;
use crate::ids::{PostId, digits};

pub type Timestamp = u64;

/// ```
/// use contracts::{Cursor, PostId};
///
/// let cursor: Cursor = "1700000000:42".parse()?;
/// assert_eq!(cursor, Cursor { time: 1_700_000_000, post: PostId(42) });
/// assert_eq!(cursor.to_string(), "1700000000:42");
///
/// let older = Cursor { time: 1_699_999_999, post: PostId(99) };
/// assert!(older < cursor);
/// # Ok::<(), contracts::ParseError>(())
/// ```
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
