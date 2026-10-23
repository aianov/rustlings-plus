use std::io;
use std::sync::{Arc, Mutex, PoisonError};

use futures::StreamExt;
use sha2::{Digest, Sha256};

use crate::store::{Chunks, FileStore};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stored {
    pub key: String,
    pub size: u64,
    pub existed: bool,
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub async fn put_content(store: &dyn FileStore, body: Chunks, temp: &str) -> io::Result<Stored> {
    let hasher = Arc::new(Mutex::new(Sha256::new()));
    let feed = Arc::clone(&hasher);
    let watched: Chunks = Box::pin(body.inspect(move |piece| {
        if let Ok(piece) = piece {
            feed.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .update(piece);
        }
    }));
    let size = store.put(temp, watched).await?;
    let digest = std::mem::take(&mut *hasher.lock().unwrap_or_else(PoisonError::into_inner));
    let key = hex(&digest.finalize());
    if store.exists(&key).await? {
        store.remove(temp).await?;
        return Ok(Stored {
            key,
            size,
            existed: true,
        });
    }
    store.rename(temp, &key).await?;
    Ok(Stored {
        key,
        size,
        existed: false,
    })
}
