use std::io;
use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};

use serde::{Deserialize, Serialize};
use social_core::{Chats, DomainError, Message, UserId};
use tokio::fs::{self, File, OpenOptions};
use tokio::io::AsyncWriteExt;

use crate::error::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Line {
    pub from: u64,
    pub to: u64,
    pub text: String,
}

#[derive(Debug)]
pub enum Refused {
    Domain(DomainError),
    Journal(io::Error),
}

pub struct Store {
    chats: Mutex<Chats>,
    journal: tokio::sync::Mutex<Option<File>>,
}

fn broken(line: usize, why: &str) -> Error {
    let message = format!("строка {line}: {why}");
    Error::Journal(io::Error::new(io::ErrorKind::InvalidData, message))
}

fn replay(text: &str) -> Result<(Chats, usize), Error> {
    let mut chats = Chats::default();
    let mut whole = 0;
    for (index, line) in text.split_inclusive('\n').enumerate() {
        if !line.ends_with('\n') {
            break;
        }
        whole += line.len();
        if line.trim().is_empty() {
            continue;
        }
        let entry: Line =
            serde_json::from_str(line).map_err(|e| broken(index + 1, &e.to_string()))?;
        let sent = chats.send(UserId(entry.from), UserId(entry.to), &entry.text);
        sent.map_err(|e| broken(index + 1, &e.to_string()))?;
    }
    Ok((chats, whole))
}

impl Store {
    pub fn in_memory() -> Store {
        Store {
            chats: Mutex::default(),
            journal: tokio::sync::Mutex::new(None),
        }
    }

    pub async fn open(path: Option<&Path>) -> Result<Store, Error> {
        let Some(path) = path else {
            return Ok(Store::in_memory());
        };
        if let Some(dir) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
            fs::create_dir_all(dir).await.map_err(Error::Journal)?;
        }
        let text = match fs::read_to_string(path).await {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(Error::Journal(error)),
        };
        let (chats, whole) = replay(&text)?;
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .await
            .map_err(Error::Journal)?;
        if whole < text.len() {
            file.set_len(whole as u64).await.map_err(Error::Journal)?;
        }
        Ok(Store {
            chats: Mutex::new(chats),
            journal: tokio::sync::Mutex::new(Some(file)),
        })
    }

    fn lock(&self) -> MutexGuard<'_, Chats> {
        self.chats.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub async fn send(&self, from: UserId, to: UserId, text: &str) -> Result<Message, Refused> {
        let text = Chats::check(from, to, text).map_err(Refused::Domain)?;
        let mut journal = self.journal.lock().await;
        if let Some(file) = journal.as_mut() {
            let line = Line {
                from: from.0,
                to: to.0,
                text: text.clone(),
            };
            let mut raw = serde_json::to_string(&line).map_err(|e| Refused::Journal(e.into()))?;
            raw.push('\n');
            file.write_all(raw.as_bytes())
                .await
                .map_err(Refused::Journal)?;
            file.flush().await.map_err(Refused::Journal)?;
        }
        let mut chats = self.lock();
        chats
            .send(from, to, &text)
            .cloned()
            .map_err(Refused::Domain)
    }

    pub fn history(&self, user: UserId, peer: UserId, limit: usize) -> Vec<Message> {
        self.lock().history(user, peer, limit).to_vec()
    }
}
