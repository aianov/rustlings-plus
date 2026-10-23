use api_gateway::Gateway;
use bytes::Bytes;
use files::Body;
use files::store::FsStore;
use futures::StreamExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use user_service::{AvatarError, AvatarService, GRACE};

struct TestDir(PathBuf);

impl TestDir {
    fn new() -> TestDir {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("svfilequiz_{}_{}_{}", std::process::id(), nanos, COUNTER.fetch_add(1, Ordering::SeqCst)));
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

struct Env {
    dir: TestDir,
    users: Arc<AvatarService<FsStore>>,
    gateway: Gateway<FsStore>,
}

fn env() -> Env {
    let dir = TestDir::new();
    let users = Arc::new(AvatarService::new(FsStore::new(dir.0.clone()), b"quiz secret"));
    let gateway = Gateway::new(users.clone());
    Env { dir, users, gateway }
}

fn png(width: u32, height: u32, seed: u8) -> Vec<u8> {
    let mut v = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, b'I', b'H', b'D', b'R'];
    v.extend_from_slice(&width.to_be_bytes());
    v.extend_from_slice(&height.to_be_bytes());
    v.extend_from_slice(&[8, 2, 0, 0, 0]);
    v.extend_from_slice(&[seed; 40]);
    v
}

fn body(data: Vec<u8>) -> Body {
    futures::stream::iter(data.chunks(64 * 1024).map(|c| Ok(Bytes::copy_from_slice(c))).collect::<Vec<_>>()).boxed()
}

fn counted(chunks: usize, size: usize) -> (Body, Arc<AtomicUsize>) {
    let polled = Arc::new(AtomicUsize::new(0));
    let seen = polled.clone();
    let stream = futures::stream::iter(0..chunks)
        .map(move |_| {
            seen.fetch_add(1, Ordering::SeqCst);
            Ok(Bytes::from(vec![9u8; size]))
        })
        .boxed();
    (stream, polled)
}

#[tokio::test]
async fn an_avatar_goes_in_and_comes_back_through_a_signed_link() {
    let env = env();
    let image = png(64, 64, 1);
    let link = env.gateway.put_avatar(1, Some(image.len() as u64), body(image.clone()), 1_000).await.expect("аватар принят");
    assert!(link.starts_with("/files/"), "ссылка на скачивание: {link}");
    let response = env.gateway.download(&link, 1_010).await;
    assert_eq!(response.status, 200, "ссылка открывается");
    assert_eq!(response.header("Content-Type"), Some("image/png"), "тип по сигнатуре");
    assert_eq!(response.header("X-Content-Type-Options"), Some("nosniff"), "nosniff в ответе");
    assert_eq!(response.body, image, "отдано то, что загружено");
}

#[tokio::test]
async fn an_oversize_declaration_is_refused_without_reading_and_a_liar_is_cut_off() {
    let env = env();
    let (source, polled) = counted(100, 64 * 1024);
    assert_eq!(env.gateway.put_avatar(1, Some(5 * 1024 * 1024), source, 0).await, Err(AvatarError::TooLarge), "заявлено 5 МиБ");
    assert_eq!(polled.load(Ordering::SeqCst), 0, "тело при этом не читали");
    let (source, polled) = counted(100, 64 * 1024);
    assert_eq!(env.gateway.put_avatar(1, Some(100), source, 0).await, Err(AvatarError::TooLarge), "заявили 100 байт, прислали мегабайты");
    assert!(polled.load(Ordering::SeqCst) <= 17, "прочитано {} кусков по 64 КиБ: лимит в один мегабайт должен остановить чтение", polled.load(Ordering::SeqCst));
    assert!(files_under(&env.dir.0).is_empty(), "на диск ничего не попало");
}

#[tokio::test]
async fn files_that_are_not_proper_images_are_refused() {
    let env = env();
    let html = b"<html><script>alert(1)</script></html>".to_vec();
    assert_eq!(env.gateway.put_avatar(1, None, body(html), 0).await, Err(AvatarError::NotImage), "HTML под видом картинки");
    assert_eq!(env.gateway.put_avatar(1, None, body(png(60_000, 60_000, 2)), 0).await, Err(AvatarError::BadSize), "картинка-бомба");
    assert_eq!(env.gateway.put_avatar(1, None, body(png(0, 10, 2)), 0).await, Err(AvatarError::BadSize), "нулевая ширина");
    assert!(files_under(&env.dir.0).is_empty(), "отказанные файлы не оставляют следов на диске");
}

#[tokio::test]
async fn identical_avatars_are_stored_once_and_the_shared_file_survives_a_change() {
    let env = env();
    let same = png(32, 32, 3);
    env.gateway.put_avatar(1, None, body(same.clone()), 10).await.unwrap();
    env.gateway.put_avatar(2, None, body(same.clone()), 11).await.unwrap();
    assert_eq!(files_under(&env.dir.0).len(), 1, "два одинаковых аватара это один файл на диске: {:?}", files_under(&env.dir.0));
    env.gateway.put_avatar(1, None, body(png(32, 32, 4)), 12).await.unwrap();
    assert_eq!(files_under(&env.dir.0).len(), 2, "общий файл остался: на него ссылается второй пользователь");
    let link = env.users.avatar_link(2, 13).expect("у второго есть аватар");
    assert_eq!(env.gateway.download(&link, 14).await.body, same, "аватар второго цел");
    env.gateway.put_avatar(2, None, body(png(32, 32, 5)), 15).await.unwrap();
    assert_eq!(files_under(&env.dir.0).len(), 2, "когда на общий файл ссылок не осталось, он удалён: остались два новых");
}

#[tokio::test]
async fn replacing_an_unshared_avatar_removes_the_old_file() {
    let env = env();
    env.gateway.put_avatar(1, None, body(png(16, 16, 6)), 10).await.unwrap();
    env.gateway.put_avatar(1, None, body(png(16, 16, 7)), 20).await.unwrap();
    assert_eq!(files_under(&env.dir.0).len(), 1, "старый файл убран, новый на месте");
    env.gateway.put_avatar(1, None, body(png(16, 16, 7)), 30).await.unwrap();
    assert_eq!(files_under(&env.dir.0).len(), 1, "загрузка того же файла повторно ничего не удаляет и не дублирует");
    let link = env.users.avatar_link(1, 31).unwrap();
    assert_eq!(env.gateway.download(&link, 32).await.status, 200, "аватар по-прежнему открывается");
}

#[tokio::test]
async fn a_failed_commit_leaves_an_orphan_that_only_the_sweep_removes_and_only_when_old() {
    let env = env();
    env.users.db.fail_commit.store(true, Ordering::SeqCst);
    assert_eq!(env.gateway.put_avatar(1, None, body(png(8, 8, 8)), 1_000).await, Err(AvatarError::Db), "база отказала");
    env.users.db.fail_commit.store(false, Ordering::SeqCst);
    assert_eq!(files_under(&env.dir.0).len(), 1, "файл уже лежит на диске");
    assert_eq!(env.users.db.pending_count(), 1, "о нём осталась запись «ожидает»");
    assert_eq!(env.users.sweep(1_000 + GRACE - 1).await, 0, "загрузка может ещё идти: рано");
    assert_eq!(files_under(&env.dir.0).len(), 1, "файл на месте");
    assert_eq!(env.users.sweep(1_000 + GRACE + 1).await, 1, "прошло время: сирота убрана");
    assert!(files_under(&env.dir.0).is_empty(), "на диске пусто");
    assert_eq!(env.users.sweep(1_000 + GRACE + 2).await, 0, "повтор ничего не находит");
}

#[tokio::test]
async fn a_sweep_never_deletes_a_file_somebody_uses() {
    let env = env();
    let image = png(8, 8, 9);
    env.gateway.put_avatar(1, None, body(image.clone()), 100).await.unwrap();
    assert_eq!(env.users.sweep(100 + GRACE * 10).await, 0, "завершённую загрузку уборщик не трогает");
    let link = env.users.avatar_link(1, 100 + GRACE * 10).unwrap();
    assert_eq!(env.gateway.download(&link, 100 + GRACE * 10 + 1).await.body, image, "аватар на месте");
}

#[tokio::test]
async fn links_expire_and_cannot_be_edited() {
    let env = env();
    let link = env.gateway.put_avatar(1, None, body(png(8, 8, 1)), 1_000).await.unwrap();
    assert_eq!(env.gateway.download(&link, 1_000 + 3_600).await.status, 410, "срок вышел");
    let forged = link.replacen("exp=", "exp=9", 1);
    assert_eq!(env.gateway.download(&forged, 1_001).await.status, 403, "срок поправлен руками");
    assert_eq!(env.gateway.download("/files/", 1_001).await.status, 400, "мусор вместо ссылки");
    let other_name = link.replace("name=avatar", "name=invoice.exe");
    assert_eq!(env.gateway.download(&other_name, 1_001).await.status, 403, "имя для скачивания подменено");
}

#[tokio::test]
async fn a_link_to_a_file_that_is_gone_is_404() {
    let env = env();
    let link = env.gateway.put_avatar(1, None, body(png(8, 8, 1)), 1_000).await.unwrap();
    for path in files_under(&env.dir.0) {
        std::fs::remove_file(path).unwrap();
    }
    assert_eq!(env.gateway.download(&link, 1_001).await.status, 404, "ссылка верная, файла нет");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn parallel_uploads_of_one_image_end_with_one_file_and_no_litter() {
    let env = env();
    let image = png(48, 48, 11);
    let env_users = env.users.clone();
    let mut tasks = Vec::new();
    for user in 1..=8u64 {
        let gateway = Gateway::new(env_users.clone());
        let image = image.clone();
        tasks.push(tokio::spawn(async move { gateway.put_avatar(user, None, body(image), 100 + user).await }));
    }
    for task in tasks {
        task.await.unwrap().expect("каждая загрузка удалась");
    }
    assert_eq!(files_under(&env.dir.0).len(), 1, "восемь одинаковых загрузок это один файл и никаких временных: {:?}", files_under(&env.dir.0));
    for user in 1..=8u64 {
        let link = env.users.avatar_link(user, 200).unwrap();
        assert_eq!(env.gateway.download(&link, 201).await.body, image, "аватар пользователя {user} открывается");
    }
}
