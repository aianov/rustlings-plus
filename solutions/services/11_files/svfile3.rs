use bytes::Bytes;
use futures::StreamExt;
use futures::stream::BoxStream;
use std::future::Future;
use std::io;
use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn main() {}

pub const CHUNK: usize = 64 * 1024;

pub type Body = BoxStream<'static, io::Result<Bytes>>;

pub trait FileStore: Send + Sync {
    fn put(&self, key: &str, body: Body) -> impl Future<Output = io::Result<u64>> + Send;
    fn get(&self, key: &str) -> impl Future<Output = io::Result<Option<Body>>> + Send;
}

pub struct FsStore {
    root: PathBuf,
}

impl FsStore {
    pub fn new(root: PathBuf) -> FsStore {
        FsStore { root }
    }

    fn path_of(&self, key: &str) -> PathBuf {
        self.root.join(key.get(..2).unwrap_or("__")).join(key)
    }
}

impl FileStore for FsStore {
    async fn put(&self, key: &str, mut body: Body) -> io::Result<u64> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = self.path_of(key);
        let dir = path.parent().expect("у пути всегда есть каталог");
        tokio::fs::create_dir_all(dir).await?;
        let temp = dir.join(format!(".{key}.{}.tmp", NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst)));
        let mut file = tokio::fs::File::create(&temp).await?;
        let mut total = 0u64;
        while let Some(chunk) = body.next().await {
            let chunk = chunk?;
            file.write_all(&chunk).await?;
            total += chunk.len() as u64;
        }
        file.flush().await?;
        file.sync_all().await?;
        drop(file);
        tokio::fs::rename(&temp, &path).await?;
        Ok(total)
    }

    async fn get(&self, key: &str) -> io::Result<Option<Body>> {
        let file = match tokio::fs::File::open(self.path_of(key)).await {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        let body = futures::stream::unfold(file, |mut file| async move {
            let mut buf = vec![0u8; CHUNK];
            match file.read(&mut buf).await {
                Ok(0) => None,
                Ok(n) => {
                    buf.truncate(n);
                    Some((Ok(Bytes::from(buf)), file))
                }
                Err(e) => Some((Err(e), file)),
            }
        });
        Ok(Some(body.boxed()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::sync::Arc;
    use tokio::sync::mpsc;
    use tokio::time::{Duration, timeout};

#[allow(dead_code)]
pub struct TestDir(pub PathBuf);

impl TestDir {
    pub fn new() -> TestDir {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("svfile_{}_{}_{}", std::process::id(), nanos, COUNTER.fetch_add(1, Ordering::SeqCst)));
        std::fs::create_dir_all(&path).unwrap();
        TestDir(path)
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[allow(dead_code)]
pub fn files_under(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

#[allow(dead_code)]
pub fn body_of(chunks: Vec<Vec<u8>>) -> Body {
    futures::stream::iter(chunks.into_iter().map(|c| Ok(Bytes::from(c)))).boxed()
}

#[allow(dead_code)]
pub async fn read_all(mut body: Body) -> io::Result<Vec<u8>> {
    let mut out = Vec::new();
    while let Some(chunk) = body.next().await {
        out.extend_from_slice(&chunk?);
    }
    Ok(out)
}

#[allow(dead_code)]
pub fn gated(rx: tokio::sync::mpsc::UnboundedReceiver<io::Result<Vec<u8>>>) -> Body {
    futures::stream::unfold(rx, |mut rx| async move { rx.recv().await.map(|item| (item.map(Bytes::from), rx)) }).boxed()
}

#[allow(dead_code)]
pub async fn wait_until(what: &str, mut check: impl FnMut() -> bool) {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    while !check() {
        assert!(tokio::time::Instant::now() < deadline, "не дождались: {what}");
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}

    fn pattern(len: usize, seed: u8) -> Vec<u8> {
        (0..len).map(|i| ((i % 251) as u8).wrapping_add(seed)).collect()
    }

    async fn read_key(store: &FsStore, key: &str) -> Option<Vec<u8>> {
        match store.get(key).await.expect("get") {
            Some(body) => Some(read_all(body).await.expect("чтение")),
            None => None,
        }
    }

    #[tokio::test]
    async fn a_reader_does_not_see_a_half_uploaded_new_file() {
        let dir = TestDir::new();
        let store = Arc::new(FsStore::new(dir.0.clone()));
        let (tx, rx) = mpsc::unbounded_channel();
        let upload = tokio::spawn({
            let store = store.clone();
            async move { store.put("ab12cd", gated(rx)).await }
        });
        tx.send(Ok(pattern(200_000, 0))).unwrap();
        wait_until("первый кусок записан на диск", || files_under(&dir.0).iter().any(|p| std::fs::metadata(p).map(|m| m.len() > 0).unwrap_or(false))).await;
        assert!(read_key(&store, "ab12cd").await.is_none(), "загрузка ещё идёт, нового файла для читателей пока нет: он должен появиться целиком или не появиться");
        tx.send(Ok(pattern(100_000, 0))).unwrap();
        drop(tx);
        assert_eq!(timeout(Duration::from_secs(5), upload).await.unwrap().unwrap().unwrap(), 300_000, "put вернул число записанных байт");
        assert_eq!(read_key(&store, "ab12cd").await.map(|v| v.len()), Some(300_000), "после конца загрузки файл виден целиком");
    }

    #[tokio::test]
    async fn while_a_replacement_uploads_readers_get_the_old_content() {
        let dir = TestDir::new();
        let store = Arc::new(FsStore::new(dir.0.clone()));
        store.put("ab12cd", body_of(vec![b"old avatar".to_vec()])).await.unwrap();
        let (tx, rx) = mpsc::unbounded_channel();
        let upload = tokio::spawn({
            let store = store.clone();
            async move { store.put("ab12cd", gated(rx)).await }
        });
        tx.send(Ok(pattern(150_000, 9))).unwrap();
        wait_until("новые байты дошли до диска", || files_under(&dir.0).iter().any(|p| std::fs::metadata(p).map(|m| m.len() >= 100_000).unwrap_or(false))).await;
        assert_eq!(read_key(&store, "ab12cd").await, Some(b"old avatar".to_vec()), "пока идёт замена, читатель видит прежний аватар целиком");
        drop(tx);
        timeout(Duration::from_secs(5), upload).await.unwrap().unwrap().unwrap();
        assert_eq!(read_key(&store, "ab12cd").await, Some(pattern(150_000, 9)), "после конца загрузки виден новый");
    }

    #[tokio::test]
    async fn a_broken_upload_changes_nothing_that_was_there() {
        let dir = TestDir::new();
        let store = Arc::new(FsStore::new(dir.0.clone()));
        store.put("ab12cd", body_of(vec![b"precious".to_vec()])).await.unwrap();
        let (tx, rx) = mpsc::unbounded_channel();
        let upload = tokio::spawn({
            let store = store.clone();
            async move { store.put("ab12cd", gated(rx)).await }
        });
        tx.send(Ok(pattern(50_000, 3))).unwrap();
        tx.send(Err(io::Error::new(io::ErrorKind::ConnectionReset, "клиент пропал"))).unwrap();
        let result = timeout(Duration::from_secs(5), upload).await.unwrap().unwrap();
        assert!(result.is_err(), "ошибка тела должна дойти до вызывающего");
        assert_eq!(read_key(&store, "ab12cd").await, Some(b"precious".to_vec()), "оборванная загрузка не портит то, что лежало под этим ключом");
    }

    #[tokio::test]
    async fn a_broken_first_upload_leaves_no_file_under_the_key() {
        let dir = TestDir::new();
        let store = FsStore::new(dir.0.clone());
        let body = futures::stream::iter(vec![Ok(Bytes::from(pattern(1000, 1))), Err(io::Error::new(io::ErrorKind::ConnectionReset, "клиент пропал"))]).boxed();
        assert!(store.put("cd34ef", body).await.is_err(), "ошибка тела должна дойти до вызывающего");
        assert!(read_key(&store, "cd34ef").await.is_none(), "от упавшей загрузки не должно остаться файла под настоящим ключом");
    }

    #[tokio::test]
    async fn the_normal_roundtrip_still_works() {
        let dir = TestDir::new();
        let store = FsStore::new(dir.0.clone());
        let data = pattern(300_000, 5);
        let size = store.put("ab12cd", body_of(data.chunks(70_000).map(<[u8]>::to_vec).collect())).await.unwrap();
        assert_eq!(size, 300_000, "put возвращает число байт");
        assert_eq!(read_key(&store, "ab12cd").await, Some(data), "прочитано то, что записано");
        assert!(dir.0.join("ab").join("ab12cd").is_file(), "итоговое место прежнее: root/ab/ab12cd");
    }
}
