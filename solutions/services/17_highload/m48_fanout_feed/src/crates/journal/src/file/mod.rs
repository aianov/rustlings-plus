use std::io;
use std::marker::PhantomData;
use std::path::Path;

use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::fs::{self, File, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

use crate::error::JournalError;
use crate::health::JournalHealth;

pub struct Journal<E> {
    file: Option<Mutex<File>>,
    health: JournalHealth,
    kind: PhantomData<fn(E)>,
}

fn replay<E: DeserializeOwned>(text: &str) -> Result<(Vec<E>, usize), JournalError> {
    let mut entries = Vec::new();
    let mut whole = 0;
    for (index, line) in text.split_inclusive('\n').enumerate() {
        if !line.ends_with('\n') {
            break;
        }
        whole += line.len();
        if line.trim().is_empty() {
            continue;
        }
        let entry = serde_json::from_str(line).map_err(|error| JournalError::Broken {
            line: index + 1,
            reason: error.to_string(),
        })?;
        entries.push(entry);
    }
    Ok((entries, whole))
}

impl<E: Serialize + DeserializeOwned> Journal<E> {
    pub fn memory() -> Journal<E> {
        Journal {
            file: None,
            health: JournalHealth::default(),
            kind: PhantomData,
        }
    }

    pub async fn open(path: Option<&Path>) -> Result<(Journal<E>, Vec<E>), JournalError> {
        let Some(path) = path else {
            return Ok((Journal::memory(), Vec::new()));
        };
        if let Some(dir) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
            fs::create_dir_all(dir).await?;
        }
        let text = match fs::read_to_string(path).await {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(error.into()),
        };
        let (entries, whole) = replay(&text)?;
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .await?;
        if whole < text.len() {
            file.set_len(whole as u64).await?;
        }
        let journal = Journal {
            file: Some(Mutex::new(file)),
            health: JournalHealth::default(),
            kind: PhantomData,
        };
        Ok((journal, entries))
    }

    pub fn is_durable(&self) -> bool {
        self.file.is_some()
    }

    pub async fn append(&self, entry: &E) -> Result<(), JournalError> {
        self.append_all(std::slice::from_ref(entry)).await
    }

    pub async fn append_all(&self, entries: &[E]) -> Result<(), JournalError> {
        let Some(file) = &self.file else {
            return Ok(());
        };
        if entries.is_empty() {
            return Ok(());
        }
        let mut line = String::new();
        for entry in entries {
            line.push_str(&serde_json::to_string(entry).map_err(JournalError::Encode)?);
            line.push('\n');
        }
        let mut file = file.lock().await;
        let written = match file.write_all(line.as_bytes()).await {
            Ok(()) => file.flush().await,
            Err(error) => Err(error),
        };
        match written {
            Ok(()) => {
                self.health.recovered();
                Ok(())
            }
            Err(error) => {
                self.health
                    .failed(format!("запись в журнал не прошла: {error}"));
                Err(error.into())
            }
        }
    }

    pub fn health(&self) -> JournalHealth {
        self.health.clone()
    }
}
