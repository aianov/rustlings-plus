use crate::{Body, FileStore};
use futures::StreamExt;
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
use std::io;

#[derive(Debug, PartialEq)]
pub struct Stored {
    pub key: String,
    pub size: u64,
    pub existed: bool,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub async fn put_content<S: FileStore>(store: &S, body: Body, temp_key: &str) -> io::Result<Stored> {
    let hasher = Arc::new(Mutex::new(Sha256::new()));
    let feed = hasher.clone();
    let hashed = body
        .inspect(move |item| {
            if let Ok(chunk) = item {
                feed.lock().unwrap().update(chunk);
            }
        })
        .boxed();
    let size = match store.put(temp_key, hashed).await {
        Ok(size) => size,
        Err(e) => {
            let _ = store.delete(temp_key).await;
            return Err(e);
        }
    };
    let key = hex(&hasher.lock().unwrap().clone().finalize());
    if store.exists(&key).await? {
        store.delete(temp_key).await?;
        return Ok(Stored { key, size, existed: true });
    }
    store.rename(temp_key, &key).await?;
    Ok(Stored { key, size, existed: false })
}
