use std::fmt;
use std::io;

use futures::StreamExt;

use crate::store::Chunks;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileError {
    TooLarge { max: u64 },
}

impl fmt::Display for FileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FileError::TooLarge { max } => write!(f, "файл больше {max} байт"),
        }
    }
}

impl std::error::Error for FileError {}

pub fn limit(body: Chunks, max: u64, declared: Option<u64>) -> Result<Chunks, FileError> {
    if declared.is_some_and(|declared| declared > max) {
        return Err(FileError::TooLarge { max });
    }
    let state = (body, 0u64, false);
    let limited = futures::stream::unfold(state, move |(mut body, seen, broken)| async move {
        if broken {
            return None;
        }
        match body.next().await? {
            Ok(piece) => {
                let seen = seen + piece.len() as u64;
                if seen > max {
                    let error =
                        io::Error::new(io::ErrorKind::InvalidData, FileError::TooLarge { max });
                    return Some((Err(error), (body, seen, true)));
                }
                Some((Ok(piece), (body, seen, false)))
            }
            Err(error) => Some((Err(error), (body, seen, true))),
        }
    });
    Ok(Box::pin(limited))
}
