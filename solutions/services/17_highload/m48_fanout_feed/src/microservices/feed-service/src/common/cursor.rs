use std::fmt;

use contracts::PostId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Cursor {
    pub at: u64,
    pub post: PostId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrokenCursor;

impl fmt::Display for BrokenCursor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("курсор ленты испорчен: возьми его из прошлого ответа как есть")
    }
}

impl std::error::Error for BrokenCursor {}

const VERSION: &str = "c1";

fn check(at: u64, post: u64) -> u32 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in at.to_be_bytes().into_iter().chain(post.to_be_bytes()) {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    u32::try_from(hash >> 32).unwrap_or(0)
}

impl Cursor {
    pub fn encode(&self) -> String {
        let post = self.post.0;
        format!(
            "{VERSION}.{:x}.{post:x}.{:08x}",
            self.at,
            check(self.at, post)
        )
    }

    pub fn decode(text: &str) -> Result<Cursor, BrokenCursor> {
        let mut parts = text.split('.');
        let (Some(version), Some(at), Some(post), Some(sum), None) = (
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
        ) else {
            return Err(BrokenCursor);
        };
        if version != VERSION || sum.len() != 8 {
            return Err(BrokenCursor);
        }
        let at = u64::from_str_radix(at, 16).map_err(|_| BrokenCursor)?;
        let post = u64::from_str_radix(post, 16).map_err(|_| BrokenCursor)?;
        let sum = u32::from_str_radix(sum, 16).map_err(|_| BrokenCursor)?;
        if sum != check(at, post) {
            return Err(BrokenCursor);
        }
        Ok(Cursor {
            at,
            post: PostId(post),
        })
    }
}
