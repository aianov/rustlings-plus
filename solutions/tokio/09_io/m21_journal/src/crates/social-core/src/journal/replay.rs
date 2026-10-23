use std::str;

use contracts::{Entry, decode};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, BufReader};

use super::JournalError;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Replayed {
    pub entries: Vec<Entry>,
    pub bytes: u64,
    pub torn: bool,
}

pub async fn replay<R: AsyncRead + Unpin>(
    source: R,
    max_line: usize,
) -> Result<Replayed, JournalError> {
    let mut reader = BufReader::new(source);
    let limit = max_line as u64 + 1;
    let mut replayed = Replayed::default();
    let mut raw = Vec::new();
    loop {
        raw.clear();
        let read = (&mut reader)
            .take(limit)
            .read_until(b'\n', &mut raw)
            .await
            .map_err(JournalError::Io)?;
        if read == 0 {
            return Ok(replayed);
        }
        let line = replayed.entries.len() + 1;
        let Some((b'\n', body)) = raw.split_last() else {
            if read as u64 >= limit {
                return Err(JournalError::TooLong {
                    line,
                    max: max_line,
                });
            }
            replayed.torn = true;
            return Ok(replayed);
        };
        let text = str::from_utf8(body).map_err(|error| JournalError::Bad {
            line,
            reason: error.to_string(),
        })?;
        let entry = decode::<Entry>(text).map_err(|error| JournalError::Bad {
            line,
            reason: error.to_string(),
        })?;
        replayed.entries.push(entry);
        replayed.bytes += read as u64;
    }
}
