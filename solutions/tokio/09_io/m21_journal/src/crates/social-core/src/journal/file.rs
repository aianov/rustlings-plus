use std::io::ErrorKind;
use std::path::Path;

use contracts::Entry;
use tokio::fs::{self, File, OpenOptions};

use super::{Journal, JournalError, replay};

pub async fn load(path: &Path, max_line: usize) -> Result<Vec<Entry>, JournalError> {
    let file = match File::open(path).await {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(JournalError::Io(error)),
    };
    let replayed = replay(file, max_line).await?;
    if replayed.torn {
        let file = OpenOptions::new()
            .write(true)
            .open(path)
            .await
            .map_err(JournalError::Io)?;
        file.set_len(replayed.bytes)
            .await
            .map_err(JournalError::Io)?;
    }
    Ok(replayed.entries)
}

impl Journal {
    pub async fn open(path: &Path) -> Result<Journal, JournalError> {
        if let Some(dir) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
            fs::create_dir_all(dir).await.map_err(JournalError::Io)?;
        }
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .await
            .map_err(JournalError::Io)?;
        Ok(Journal::new(file))
    }
}
