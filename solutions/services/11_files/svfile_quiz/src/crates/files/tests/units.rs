use bytes::Bytes;
use files::content::put_content;
use files::image::{AvatarError, Kind, check_avatar};
use files::limit::{TooLarge, limit};
use files::serve::{Meta, serve};
use files::sign::{LinkError, Signer};
use files::store::FsStore;
use files::{Body, FileStore};
use futures::StreamExt;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

struct TestDir(PathBuf);

impl TestDir {
    fn new() -> TestDir {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("svfileq_{}_{}_{}", std::process::id(), nanos, COUNTER.fetch_add(1, Ordering::SeqCst)));
        std::fs::create_dir_all(&path).unwrap();
        TestDir(path)
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn files_under(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() { stack.push(path) } else { found.push(path) }
        }
    }
    found.sort();
    found
}

fn body_of(chunks: Vec<Vec<u8>>) -> Body {
    futures::stream::iter(chunks.into_iter().map(|c| Ok(Bytes::from(c)))).boxed()
}

fn gated(rx: mpsc::UnboundedReceiver<io::Result<Vec<u8>>>) -> Body {
    futures::stream::unfold(rx, |mut rx| async move { rx.recv().await.map(|item| (item.map(Bytes::from), rx)) }).boxed()
}

async fn read_key(store: &FsStore, key: &str) -> Option<Vec<u8>> {
    let mut body = store.get(key).await.expect("get")?;
    let mut out = Vec::new();
    while let Some(chunk) = body.next().await {
        out.extend_from_slice(&chunk.expect("чтение"));
    }
    Some(out)
}

async fn wait_until(what: &str, mut check: impl FnMut() -> bool) {
    for _ in 0..500 {
        if check() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("не дождались: {what}");
}

fn png(width: u32, height: u32) -> Vec<u8> {
    let mut v = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, b'I', b'H', b'D', b'R'];
    v.extend_from_slice(&width.to_be_bytes());
    v.extend_from_slice(&height.to_be_bytes());
    v.extend_from_slice(&[8, 2, 0, 0, 0, 0, 0, 0, 0]);
    v
}

fn jpeg_with_decoy(width: u16, height: u16) -> Vec<u8> {
    let decoy = [0xFF, 0xC0, 0x00, 0x11, 0x08, 0x00, 0x10, 0x00, 0x10, 0x03];
    let mut v = vec![0xFF, 0xD8, 0xFF, 0xE0];
    v.extend_from_slice(&((decoy.len() + 2) as u16).to_be_bytes());
    v.extend_from_slice(&decoy);
    v.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11, 0x08]);
    v.extend_from_slice(&height.to_be_bytes());
    v.extend_from_slice(&width.to_be_bytes());
    v.extend_from_slice(&[3, 1, 0x22, 0, 2, 0x11, 1, 3, 0x11, 1, 0xFF, 0xD9]);
    v
}

#[tokio::test]
async fn limit_counts_on_the_fly_and_trusts_no_header() {
    let polled = Arc::new(AtomicUsize::new(0));
    let seen = polled.clone();
    let source = futures::stream::iter(0..1000)
        .map(move |_| {
            seen.fetch_add(1, Ordering::SeqCst);
            Ok(Bytes::from(vec![1u8; 1000]))
        })
        .boxed();
    let mut limited = limit(source, 3500, Some(10)).expect("заявленный размер маленький");
    let mut bytes = 0;
    let mut failed = None;
    while let Some(item) = limited.next().await {
        match item {
            Ok(chunk) => bytes += chunk.len(),
            Err(e) => {
                failed = Some(e.kind());
                break;
            }
        }
    }
    assert_eq!((bytes, failed), (3000, Some(io::ErrorKind::FileTooLarge)), "три куска прошли, четвёртый вывел за предел");
    assert!(polled.load(Ordering::SeqCst) <= 4, "источник прочитан {} раз: считать надо по ходу", polled.load(Ordering::SeqCst));
    assert!(limited.next().await.is_none(), "после ошибки поток закончился");
    let untouched = Arc::new(AtomicUsize::new(0));
    let probe = untouched.clone();
    let source = futures::stream::iter(0..3).map(move |_| {
        probe.fetch_add(1, Ordering::SeqCst);
        Ok(Bytes::new())
    });
    assert!(matches!(limit(source.boxed(), 100, Some(101)), Err(TooLarge { limit: 100 })), "заявлено больше предела: отказ сразу");
    assert_eq!(untouched.load(Ordering::SeqCst), 0, "и без чтения тела");
}

#[tokio::test]
async fn a_reader_never_sees_half_an_upload_and_a_broken_one_changes_nothing() {
    let dir = TestDir::new();
    let store = Arc::new(FsStore::new(dir.0.clone()));
    store.put("ab12cd", body_of(vec![b"old".to_vec()])).await.unwrap();
    let (tx, rx) = mpsc::unbounded_channel();
    let upload = tokio::spawn({
        let store = store.clone();
        async move { store.put("ab12cd", gated(rx)).await }
    });
    tx.send(Ok(vec![5u8; 150_000])).unwrap();
    wait_until("новые байты на диске", || files_under(&dir.0).iter().any(|p| std::fs::metadata(p).map(|m| m.len() >= 100_000).unwrap_or(false))).await;
    assert_eq!(read_key(&store, "ab12cd").await, Some(b"old".to_vec()), "пока идёт замена, читатель видит прежнее содержимое целиком");
    tx.send(Err(io::Error::new(io::ErrorKind::ConnectionReset, "обрыв"))).unwrap();
    assert!(upload.await.unwrap().is_err(), "ошибка тела доходит до вызывающего");
    assert_eq!(read_key(&store, "ab12cd").await, Some(b"old".to_vec()), "оборванная загрузка ничего не меняет");
    assert_eq!(files_under(&dir.0), vec![dir.0.join("ab").join("ab12cd")], "и не оставляет временных файлов");
}

#[tokio::test]
async fn a_cancelled_upload_leaves_no_temporary_file() {
    let dir = TestDir::new();
    let store = Arc::new(FsStore::new(dir.0.clone()));
    let (tx, rx) = mpsc::unbounded_channel();
    let upload = tokio::spawn({
        let store = store.clone();
        async move { store.put("cd34ef", gated(rx)).await }
    });
    tx.send(Ok(vec![1u8; 100_000])).unwrap();
    wait_until("временный файл с данными", || files_under(&dir.0).iter().any(|p| std::fs::metadata(p).map(|m| m.len() > 0).unwrap_or(false))).await;
    upload.abort();
    assert!(upload.await.unwrap_err().is_cancelled(), "задачу отменили");
    assert!(files_under(&dir.0).is_empty(), "после отмены на диске пусто: {:?}", files_under(&dir.0));
    drop(tx);
}

#[test]
fn image_checks_read_headers_instead_of_trusting_sizes() {
    assert_eq!(check_avatar(&png(256, 256)), Ok(Kind::Png), "обычный аватар");
    assert_eq!(check_avatar(&png(60_000, 60_000)), Err(AvatarError::BadSize { width: 60_000, height: 60_000 }), "бомба в сотню байт");
    assert!(matches!(check_avatar(&png(65_536, 65_536)), Err(AvatarError::BadSize { .. })), "площадь не должна переполняться");
    assert_eq!(check_avatar(&jpeg_with_decoy(40_000, 40_000)), Err(AvatarError::BadSize { width: 40_000, height: 40_000 }), "размер JPEG после чужого сегмента с похожими байтами");
    assert_eq!(check_avatar(b"<html><script></script></html>"), Err(AvatarError::NotImage), "HTML не картинка");
    assert_eq!(check_avatar(&png(10, 10)[..20]), Err(AvatarError::Truncated), "оборванный заголовок");
}

#[tokio::test]
async fn equal_content_is_stored_once_under_its_sha256() {
    let dir = TestDir::new();
    let store = FsStore::new(dir.0.clone());
    let first = put_content(&store, body_of(vec![b"a".to_vec(), b"bc".to_vec()]), "tmp-1").await.unwrap();
    let second = put_content(&store, body_of(vec![b"abc".to_vec()]), "tmp-2").await.unwrap();
    assert_eq!(first.key, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad", "ключ это sha256 в hex");
    assert!(!first.existed && second.existed, "вторая загрузка узнала, что файл уже есть");
    assert_eq!(files_under(&dir.0).len(), 1, "на диске одна копия, без временных: {:?}", files_under(&dir.0));
    let broken = futures::stream::iter(vec![Ok(Bytes::from_static(b"half")), Err(io::Error::new(io::ErrorKind::ConnectionReset, "обрыв"))]).boxed();
    assert!(put_content(&store, broken, "tmp-3").await.is_err(), "ошибка тела");
    assert_eq!(files_under(&dir.0).len(), 1, "оборванная загрузка ничего не оставила");
}

#[test]
fn links_cover_every_parameter_and_check_the_signature_first() {
    let signer = Signer::new(b"secret");
    let link = signer.link("ab12cd", 2_000, "avatar");
    let sig = link.rsplit("&sig=").next().unwrap().to_string();
    assert_eq!(signer.verify(&link, 1_000).map(|o| o.key), Ok("ab12cd".to_string()), "честная ссылка");
    assert_eq!(signer.verify(&link, 2_000), Err(LinkError::Expired), "в момент срока уже нет");
    assert_eq!(signer.verify(&format!("/files/ab12cd?exp=9999999&name=avatar&sig={sig}"), 1_000), Err(LinkError::BadSignature), "срок продлили вручную");
    assert_eq!(signer.verify(&format!("/files/ab12cd?exp=2000&name=invoice.exe&sig={sig}"), 1_000), Err(LinkError::BadSignature), "имя подменили");
    assert_eq!(signer.verify(&format!("/files/ab12cd?exp=5&name=avatar&sig={}", "0".repeat(64)), 1_000), Err(LinkError::BadSignature), "подделка с прошлым сроком не должна выглядеть как просрочка");
    let link = signer.link("ab", 12, "3.png");
    let shifted = format!("/files/ab1?exp=2&name=3.png&sig={}", link.rsplit("&sig=").next().unwrap());
    assert_eq!(signer.verify(&shifted, 1), Err(LinkError::BadSignature), "поля разделены, сдвинуть границу нельзя");
    assert_eq!(signer.verify("/files/", 1), Err(LinkError::Malformed), "мусор");
}

#[test]
fn serving_never_lets_the_client_pick_the_type_or_the_headers() {
    let page = serve(&Meta { client_name: "me.png".into() }, b"<html><script>alert(1)</script></html>");
    assert_eq!(page.header("Content-Type"), Some("application/octet-stream"), "HTML отдаётся как набор байт");
    assert!(page.header("Content-Disposition").unwrap().starts_with("attachment"), "и как скачивание");
    assert_eq!(page.header("X-Content-Type-Options"), Some("nosniff"), "nosniff");
    let image = serve(&Meta { client_name: "x.png\r\nSet-Cookie: s=1\"".into() }, &png(4, 4));
    assert_eq!(image.header("Content-Type"), Some("image/png"), "настоящий PNG");
    assert!(image.header("Content-Disposition").unwrap().starts_with("inline"), "картинка показывается");
    for (name, value) in &image.headers {
        assert!(!value.contains('\r') && !value.contains('\n'), "перевод строки в {name}: {value:?}");
    }
    assert_eq!(image.header("Content-Disposition").unwrap().matches('"').count(), 2, "кавычка из имени не вырвалась");
}

