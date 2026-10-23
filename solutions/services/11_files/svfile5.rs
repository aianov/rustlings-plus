use bytes::Bytes;
use futures::StreamExt;
use futures::stream::BoxStream;
use std::future::Future;
use std::io;
use std::path::PathBuf;
use tokio::io::AsyncWriteExt;

fn main() {}

pub type Body = BoxStream<'static, io::Result<Bytes>>;

pub trait FileStore: Send + Sync {
    fn put(&self, key: &str, body: Body) -> impl Future<Output = io::Result<u64>> + Send;
}

pub struct FsStore {
    root: PathBuf,
}

struct TempGuard {
    path: PathBuf,
    armed: bool,
}

impl TempGuard {
    fn new(path: PathBuf) -> TempGuard {
        TempGuard { path, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for TempGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = std::fs::remove_file(&self.path);
        }
    }
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
        let mut guard = TempGuard::new(temp.clone());
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
        guard.disarm();
        Ok(total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
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

    #[tokio::test]
    async fn a_successful_upload_leaves_exactly_the_final_file() {
        let dir = TestDir::new();
        let store = FsStore::new(dir.0.clone());
        store.put("ab12cd", body_of(vec![vec![1; 1000], vec![2; 1000]])).await.unwrap();
        assert_eq!(files_under(&dir.0), vec![dir.0.join("ab").join("ab12cd")], "после успеха на диске только итоговый файл, а не он и временный");
        assert_eq!(std::fs::metadata(dir.0.join("ab").join("ab12cd")).unwrap().len(), 2000, "и он не пустой: страж не должен убрать результат");
    }

    #[tokio::test]
    async fn a_failing_body_leaves_no_litter() {
        let dir = TestDir::new();
        let store = FsStore::new(dir.0.clone());
        let body = futures::stream::iter(vec![Ok(Bytes::from(vec![9u8; 5000])), Err(io::Error::new(io::ErrorKind::ConnectionReset, "клиент пропал"))]).boxed();
        assert!(store.put("cd34ef", body).await.is_err(), "ошибка тела доходит до вызывающего");
        assert_eq!(files_under(&dir.0), Vec::<PathBuf>::new(), "после ошибки тела временный файл должен исчезнуть");
    }

    #[tokio::test]
    async fn a_cancelled_upload_leaves_no_litter() {
        let dir = TestDir::new();
        let store = Arc::new(FsStore::new(dir.0.clone()));
        let (tx, rx) = mpsc::unbounded_channel();
        let upload = tokio::spawn({
            let store = store.clone();
            async move { store.put("ab12cd", gated(rx)).await }
        });
        tx.send(Ok(vec![5u8; 100_000])).unwrap();
        wait_until("временный файл появился и в нём есть данные", || files_under(&dir.0).iter().any(|p| std::fs::metadata(p).map(|m| m.len() > 0).unwrap_or(false))).await;
        upload.abort();
        let joined = timeout(Duration::from_secs(5), upload).await.expect("задача должна завершиться");
        assert!(joined.unwrap_err().is_cancelled(), "задачу отменили на середине");
        assert_eq!(files_under(&dir.0), Vec::<PathBuf>::new(), "отмена на середине (клиент закрыл вкладку) тоже не оставляет временный файл: ошибочная ветка тут не выполняется, убирать должен Drop");
        drop(tx);
    }

    #[tokio::test]
    async fn an_upload_over_an_existing_file_keeps_it_when_cancelled() {
        let dir = TestDir::new();
        let store = Arc::new(FsStore::new(dir.0.clone()));
        store.put("ab12cd", body_of(vec![b"first".to_vec()])).await.unwrap();
        let (tx, rx) = mpsc::unbounded_channel();
        let upload = tokio::spawn({
            let store = store.clone();
            async move { store.put("ab12cd", gated(rx)).await }
        });
        tx.send(Ok(vec![5u8; 100_000])).unwrap();
        wait_until("временный файл появился", || files_under(&dir.0).len() == 2).await;
        upload.abort();
        let _ = timeout(Duration::from_secs(5), upload).await.expect("задача должна завершиться");
        assert_eq!(files_under(&dir.0), vec![dir.0.join("ab").join("ab12cd")], "остался только прежний итоговый файл");
        assert_eq!(std::fs::read(dir.0.join("ab").join("ab12cd")).unwrap(), b"first", "и его содержимое не тронуто");
        drop(tx);
    }
}
