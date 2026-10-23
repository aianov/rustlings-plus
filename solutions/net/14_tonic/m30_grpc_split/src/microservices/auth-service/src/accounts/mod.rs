use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};

use serde::{Deserialize, Serialize};
use tokio::fs::{self, File, OpenOptions};
use tokio::io::AsyncWriteExt;

use crate::error::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub user_id: u64,
    pub username: String,
}

#[derive(Default)]
struct Known {
    by_id: HashMap<u64, Account>,
    by_name: HashMap<String, u64>,
}

impl Known {
    fn insert(&mut self, account: Account) {
        self.by_name
            .insert(account.username.to_lowercase(), account.user_id);
        self.by_id.insert(account.user_id, account);
    }
}

pub struct Accounts {
    known: Mutex<Known>,
    journal: Option<tokio::sync::Mutex<File>>,
}

fn broken(line: usize, error: &serde_json::Error) -> Error {
    let message = format!("строка {line}: {error}");
    Error::Journal(io::Error::new(io::ErrorKind::InvalidData, message))
}

impl Accounts {
    pub fn in_memory() -> Accounts {
        Accounts {
            known: Mutex::default(),
            journal: None,
        }
    }

    pub async fn open(path: Option<&Path>) -> Result<Accounts, Error> {
        let Some(path) = path else {
            return Ok(Accounts::in_memory());
        };
        if let Some(dir) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
            fs::create_dir_all(dir).await.map_err(Error::Journal)?;
        }
        let text = match fs::read_to_string(path).await {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(Error::Journal(error)),
        };
        let mut known = Known::default();
        let mut whole = 0;
        for (index, line) in text.split_inclusive('\n').enumerate() {
            if !line.ends_with('\n') {
                break;
            }
            whole += line.len();
            if line.trim().is_empty() {
                continue;
            }
            let account = serde_json::from_str(line).map_err(|e| broken(index + 1, &e))?;
            known.insert(account);
        }
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .await
            .map_err(Error::Journal)?;
        if whole < text.len() {
            file.set_len(whole as u64).await.map_err(Error::Journal)?;
        }
        Ok(Accounts {
            known: Mutex::new(known),
            journal: Some(tokio::sync::Mutex::new(file)),
        })
    }

    fn lock(&self) -> MutexGuard<'_, Known> {
        self.known.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub async fn add(&self, account: Account) -> io::Result<()> {
        if let Some(journal) = &self.journal {
            let mut line = serde_json::to_string(&account).map_err(io::Error::other)?;
            line.push('\n');
            let mut file = journal.lock().await;
            file.write_all(line.as_bytes()).await?;
            file.flush().await?;
        }
        self.lock().insert(account);
        Ok(())
    }

    pub fn find(&self, username: &str) -> Option<Account> {
        let known = self.lock();
        let id = known.by_name.get(&username.to_lowercase())?;
        known.by_id.get(id).cloned()
    }

    pub fn get(&self, user_id: u64) -> Option<Account> {
        self.lock().by_id.get(&user_id).cloned()
    }

    pub fn len(&self) -> usize {
        self.lock().by_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
