// Аватары, вложения и экспорты лежат на диске, но остальной сервис не должен знать пути: он зовёт «положи под ключом»,
// «дай по ключу», «удали». Это порт `FileStore`, и его первая реализация это каталог на диске. Тело файла приходит и
// уходит потоком кусков: файл в 200 МБ нельзя читать в память целиком. Цель: `FsStore` кладёт файл по ключу (в
// подкаталог из первых двух символов ключа, чтобы каталог не разрастался), отдаёт потоком кусков не больше `CHUNK`
// байт, говорит «нет такого» вместо ошибки и честно сообщает, удалил ли что-нибудь.

use bytes::Bytes;
use futures::StreamExt;
use futures::stream::BoxStream;
use std::future::Future;
use std::io;
use std::path::PathBuf;

fn main() {}

pub const CHUNK: usize = 64 * 1024;

pub type Body = BoxStream<'static, io::Result<Bytes>>;

pub trait FileStore: Send + Sync {
    fn put(&self, key: &str, body: Body) -> impl Future<Output = io::Result<u64>> + Send;
    fn get(&self, key: &str) -> impl Future<Output = io::Result<Option<Body>>> + Send;
    fn delete(&self, key: &str) -> impl Future<Output = io::Result<bool>> + Send;
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
        let _ = (key, &mut body, self.path_of(key));
        todo!()
    }

    async fn get(&self, key: &str) -> io::Result<Option<Body>> {
        let _ = (key, CHUNK);
        todo!()
    }

    async fn delete(&self, key: &str) -> io::Result<bool> {
        let _ = key;
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

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

    fn pattern(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    #[tokio::test]
    async fn a_file_goes_in_and_comes_out_unchanged() {
        let dir = TestDir::new();
        let store = FsStore::new(dir.0.clone());
        let data = pattern(300_000);
        let chunks: Vec<Vec<u8>> = data.chunks(70_000).map(<[u8]>::to_vec).collect();
        let size = store.put("ab12cd", body_of(chunks)).await.expect("put");
        assert_eq!(size, 300_000, "put возвращает число записанных байт");
        let body = store.get("ab12cd").await.expect("get").expect("файл есть");
        assert_eq!(read_all(body).await.unwrap(), data, "прочитано ровно то, что записано");
    }

    #[tokio::test]
    async fn reading_is_streamed_in_small_pieces() {
        let dir = TestDir::new();
        let store = FsStore::new(dir.0.clone());
        store.put("ff00", body_of(vec![pattern(500_000)])).await.unwrap();
        let mut body = store.get("ff00").await.unwrap().unwrap();
        let (mut pieces, mut biggest) = (0, 0);
        while let Some(chunk) = body.next().await {
            let chunk = chunk.unwrap();
            pieces += 1;
            biggest = biggest.max(chunk.len());
        }
        assert!(biggest <= CHUNK, "кусок в {biggest} байт: файл читается целиком, а нужно кусками не больше {CHUNK}");
        assert!(pieces >= 7, "500 000 байт кусками по 64 КиБ это не меньше семи кусков, а получилось {pieces}");
    }

    #[tokio::test]
    async fn missing_files_are_a_normal_answer_not_an_error() {
        let dir = TestDir::new();
        let store = FsStore::new(dir.0.clone());
        assert!(store.get("00none").await.expect("отсутствие файла не ошибка").is_none(), "нет такого файла: None");
        assert!(!store.delete("00none").await.expect("delete"), "удалять было нечего: false");
        store.put("00none", body_of(vec![b"x".to_vec()])).await.unwrap();
        assert!(store.delete("00none").await.unwrap(), "удалили настоящий файл: true");
        assert!(store.get("00none").await.unwrap().is_none(), "после удаления файла нет");
    }

    #[tokio::test]
    async fn the_key_decides_the_place_not_the_caller() {
        let dir = TestDir::new();
        let store = FsStore::new(dir.0.clone());
        store.put("ab12cd", body_of(vec![b"data".to_vec()])).await.unwrap();
        assert!(dir.0.join("ab").join("ab12cd").is_file(), "файл лежит в подкаталоге из первых двух символов ключа: root/ab/ab12cd");
    }

    #[tokio::test]
    async fn putting_again_replaces_the_content_completely() {
        let dir = TestDir::new();
        let store = FsStore::new(dir.0.clone());
        store.put("ab12cd", body_of(vec![pattern(10_000)])).await.unwrap();
        store.put("ab12cd", body_of(vec![b"short".to_vec()])).await.unwrap();
        let body = store.get("ab12cd").await.unwrap().unwrap();
        assert_eq!(read_all(body).await.unwrap(), b"short", "новое содержимое заменило старое целиком, хвост старого не остался");
    }
}
