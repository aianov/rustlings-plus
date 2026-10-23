use bytes::Bytes;
use futures::StreamExt;
use futures::stream::BoxStream;
use sha2::{Digest, Sha256};
use std::future::Future;
use std::io;
use std::sync::{Arc, Mutex};

fn main() {}

pub type Body = BoxStream<'static, io::Result<Bytes>>;

pub trait FileStore: Send + Sync {
    fn put(&self, key: &str, body: Body) -> impl Future<Output = io::Result<u64>> + Send;
    fn exists(&self, key: &str) -> impl Future<Output = io::Result<bool>> + Send;
    fn rename(&self, from: &str, to: &str) -> impl Future<Output = io::Result<()>> + Send;
    fn delete(&self, key: &str) -> impl Future<Output = io::Result<bool>> + Send;
}

#[derive(Debug, PartialEq)]
pub struct Stored {
    pub key: String,
    pub size: u64,
    pub existed: bool,
}

pub fn hex(bytes: &[u8]) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Default)]
    struct MemStore {
        files: Mutex<HashMap<String, Vec<u8>>>,
        probe: Option<Arc<AtomicUsize>>,
        first_chunk_at: Mutex<Option<usize>>,
    }

    impl MemStore {
        fn keys(&self) -> Vec<String> {
            let mut keys: Vec<String> = self.files.lock().unwrap().keys().cloned().collect();
            keys.sort();
            keys
        }
    }

    impl FileStore for MemStore {
        async fn put(&self, key: &str, mut body: Body) -> io::Result<u64> {
            let mut data = Vec::new();
            while let Some(chunk) = body.next().await {
                let chunk = match chunk {
                    Ok(chunk) => chunk,
                    Err(e) => {
                        self.files.lock().unwrap().insert(key.to_string(), data);
                        return Err(e);
                    }
                };
                if let Some(probe) = &self.probe {
                    self.first_chunk_at.lock().unwrap().get_or_insert(probe.load(Ordering::SeqCst));
                }
                data.extend_from_slice(&chunk);
            }
            let len = data.len() as u64;
            self.files.lock().unwrap().insert(key.to_string(), data);
            Ok(len)
        }

        async fn exists(&self, key: &str) -> io::Result<bool> {
            Ok(self.files.lock().unwrap().contains_key(key))
        }

        async fn rename(&self, from: &str, to: &str) -> io::Result<()> {
            let mut files = self.files.lock().unwrap();
            let data = files.remove(from).ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "нет такого ключа"))?;
            files.insert(to.to_string(), data);
            Ok(())
        }

        async fn delete(&self, key: &str) -> io::Result<bool> {
            Ok(self.files.lock().unwrap().remove(key).is_some())
        }
    }

    fn body(chunks: &[&[u8]]) -> Body {
        let owned: Vec<Bytes> = chunks.iter().map(|c| Bytes::copy_from_slice(c)).collect();
        futures::stream::iter(owned.into_iter().map(Ok)).boxed()
    }

    const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    #[tokio::test]
    async fn the_key_is_the_sha256_of_the_content_in_hex() {
        let store = MemStore::default();
        let stored = put_content(&store, body(&[b"abc"]), "tmp-1").await.unwrap();
        assert_eq!(stored, Stored { key: ABC.to_string(), size: 3, existed: false }, "ключ это sha256 содержимого, маленькими hex-буквами");
        assert_eq!(store.keys(), vec![ABC.to_string()], "в хранилище только файл под ключом-хешем, временного ключа нет");
    }

    #[tokio::test]
    async fn chunk_boundaries_do_not_change_the_key() {
        let store = MemStore::default();
        let whole = put_content(&store, body(&[b"abc"]), "tmp-1").await.unwrap();
        let pieces = put_content(&store, body(&[b"a", b"", b"b", b"c"]), "tmp-2").await.unwrap();
        assert_eq!(whole.key, pieces.key, "ключ зависит от байтов, а не от того, как их нарезали на куски");
    }

    #[tokio::test]
    async fn identical_files_are_stored_once() {
        let store = MemStore::default();
        let first = put_content(&store, body(&[b"same ", b"bytes"]), "tmp-1").await.unwrap();
        let second = put_content(&store, body(&[b"same bytes"]), "tmp-2").await.unwrap();
        assert!(!first.existed, "первая загрузка новая");
        assert!(second.existed, "вторая с тем же содержимым: файл уже был");
        assert_eq!(first.key, second.key, "ключи совпали");
        assert_eq!(store.keys().len(), 1, "в хранилище одна копия, а не две и не временный файл: {:?}", store.keys());
    }

    #[tokio::test]
    async fn different_files_get_different_keys() {
        let store = MemStore::default();
        let a = put_content(&store, body(&[b"aaa"]), "tmp-1").await.unwrap();
        let b = put_content(&store, body(&[b"aab"]), "tmp-2").await.unwrap();
        assert_ne!(a.key, b.key, "разное содержимое, разные ключи");
        assert_eq!(store.keys().len(), 2, "оба файла лежат");
    }

    #[tokio::test]
    async fn the_hash_is_computed_while_streaming() {
        let produced = Arc::new(AtomicUsize::new(0));
        let counter = produced.clone();
        let source = futures::stream::iter(0..100).map(move |i| {
            counter.fetch_add(1, Ordering::SeqCst);
            Ok(Bytes::from(vec![i as u8; 1000]))
        });
        let store = MemStore { probe: Some(produced.clone()), ..MemStore::default() };
        put_content(&store, source.boxed(), "tmp-1").await.unwrap();
        let at = store.first_chunk_at.lock().unwrap().expect("хранилище получило куски");
        assert!(at <= 2, "первый кусок дошёл до хранилища, когда источник отдал уже {at} кусков из 100: тело сначала собирают целиком в память, а надо передавать дальше по ходу");
    }

    #[tokio::test]
    async fn a_broken_body_leaves_nothing_behind() {
        let store = MemStore::default();
        let source = futures::stream::iter(vec![Ok(Bytes::from_static(b"half")), Err(io::Error::new(io::ErrorKind::ConnectionReset, "обрыв"))]).boxed();
        assert!(put_content(&store, source, "tmp-1").await.is_err(), "ошибка тела доходит до вызывающего");
        assert!(store.keys().is_empty(), "от оборванной загрузки не остаётся ни ключа-хеша, ни временного файла: {:?}", store.keys());
    }

    #[tokio::test]
    async fn an_empty_file_is_a_file_too() {
        let store = MemStore::default();
        let stored = put_content(&store, body(&[]), "tmp-1").await.unwrap();
        assert_eq!(stored.key, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", "sha256 пустого тела");
        assert_eq!(stored.size, 0, "размер ноль");
    }
}
