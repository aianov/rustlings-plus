use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use contracts::{Origin, UserId, Username};
use files::{Chunks, FileStore, FsFileStore, chunks_of};
use proto::AVATAR_MAX;
use user_service::repository::AvatarRepository;
use user_service::{MemoryAvatars, MemoryUsers, UserError, UserService};

fn png(width: u32, height: u32, size: usize) -> Vec<u8> {
    let mut data = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
    data.extend_from_slice(&width.to_be_bytes());
    data.extend_from_slice(&height.to_be_bytes());
    data.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
    let head = data.len();
    data.extend((head..size.max(head)).map(|i| (i % 251) as u8));
    data
}

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("m43-users")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("папка для теста создаётся");
    dir
}

fn files_in(root: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                found.push(entry.file_name().to_string_lossy().to_string());
            }
        }
    }
    found.sort();
    found
}

struct Setup {
    service: Arc<UserService>,
    store: Arc<FsFileStore>,
    avatars: Arc<MemoryAvatars>,
    root: PathBuf,
    alice: UserId,
}

async fn setup(name: &str) -> Setup {
    let root = scratch(name);
    let store = Arc::new(FsFileStore::new(&root));
    let avatars = Arc::new(MemoryAvatars::default());
    let service = UserService::new(
        Arc::new(MemoryUsers::new()),
        Arc::clone(&avatars) as Arc<dyn AvatarRepository>,
    )
    .with_files(Arc::clone(&store) as Arc<dyn FileStore>);
    let service = Arc::new(service);
    let alice = service
        .register(
            Username::try_from("alice").expect("ник"),
            &Origin::new("m43"),
        )
        .await
        .expect("alice регистрируется")
        .id;
    Setup {
        service,
        store,
        avatars,
        root,
        alice,
    }
}

fn body(data: &[u8]) -> Chunks {
    chunks_of(data.to_vec(), 16 * 1024)
}

#[tokio::test]
async fn avatar_is_stored_by_content() {
    let Setup {
        service,
        root,
        alice,
        ..
    } = setup("content").await;
    let picture = png(64, 64, 100_000);
    let (avatar, created) = service
        .put_avatar(alice, body(&picture), Some(picture.len() as u64))
        .await
        .expect("аватар принят");
    assert!(created);
    assert_eq!(avatar.content_type, "image/png", "тип по сигнатуре");
    assert_eq!(
        avatar.key.len(),
        64,
        "ключ это sha256 в hex: {}",
        avatar.key
    );
    let (_, data) = service.avatar(alice).await.expect("аватар читается");
    assert_eq!(data, picture);
    assert_eq!(
        service.file(&avatar.key).await.expect("файл по ключу"),
        picture
    );
    assert_eq!(
        files_in(&root),
        vec![avatar.key.clone()],
        "на диске один файл без временных"
    );
    let (same, created) = service
        .put_avatar(alice, body(&picture), None)
        .await
        .expect("та же картинка");
    assert_eq!((same.key.as_str(), created), (avatar.key.as_str(), false));
    let other = png(32, 32, 5_000);
    let (next, _) = service
        .put_avatar(alice, body(&other), None)
        .await
        .expect("другая картинка");
    assert_ne!(next.key, avatar.key);
    assert_eq!(
        files_in(&root),
        vec![next.key.clone()],
        "старый файл без ссылок удалён"
    );
    assert!(service.delete_avatar(alice).await);
    assert!(files_in(&root).is_empty(), "после удаления файлов нет");
    assert!(matches!(
        service.file(&next.key).await,
        Err(UserError::NoFile(..))
    ));
}

#[tokio::test]
async fn bad_uploads_leave_nothing_behind() {
    let Setup {
        service,
        root,
        alice,
        ..
    } = setup("bad").await;
    let early = service
        .put_avatar(alice, body(&[0; 10]), Some(AVATAR_MAX as u64 + 1))
        .await;
    assert!(
        matches!(early, Err(UserError::TooLarge(..))),
        "заявленный размер больше предела: отказ до чтения {early:?}"
    );
    let huge = png(64, 64, AVATAR_MAX + 1);
    let late = service.put_avatar(alice, body(&huge), None).await;
    assert!(
        matches!(late, Err(UserError::TooLarge(..))),
        "без заявленного размера предел ловится на лету {late:?}"
    );
    let text = service
        .put_avatar(alice, body(b"<svg onload=alert(1)>"), None)
        .await;
    assert!(matches!(text, Err(UserError::NotImage(..))), "{text:?}");
    let giant = service
        .put_avatar(alice, body(&png(5000, 5000, 64)), None)
        .await;
    assert!(matches!(giant, Err(UserError::NotImage(..))), "{giant:?}");
    let pieces: Vec<io::Result<Vec<u8>>> = vec![
        Ok(png(8, 8, 40_000)),
        Err(io::Error::new(io::ErrorKind::ConnectionReset, "обрыв")),
    ];
    let cut = service
        .put_avatar(alice, Box::pin(futures::stream::iter(pieces)), None)
        .await;
    assert!(cut.is_err(), "оборванная загрузка не принята");
    assert!(
        files_in(&root).is_empty(),
        "ни временных, ни недописанных файлов: {:?}",
        files_in(&root)
    );
    assert!(service.avatar_of(alice).await.is_none());
    let ghost = service
        .put_avatar(UserId(99), body(&png(8, 8, 100)), None)
        .await;
    assert!(
        matches!(ghost, Err(UserError::UnknownUser(..))),
        "{ghost:?}"
    );
}

#[tokio::test]
async fn orphans_are_swept_after_grace() {
    let Setup {
        service,
        store,
        avatars,
        root,
        alice,
    } = setup("orphans").await;
    let picture = png(16, 16, 2_000);
    let (avatar, _) = service
        .put_avatar(alice, body(&picture), None)
        .await
        .expect("аватар");
    let now = SystemClock::secs();
    store
        .put("orphan-file", body(b"left by a crash"))
        .await
        .expect("сирота пишется");
    avatars.pending(String::from("orphan-file"), now).await;
    assert_eq!(
        service.sweep(now, Duration::from_secs(3600)).await,
        0,
        "свежая незавершённая загрузка ждёт срока"
    );
    assert_eq!(
        service.sweep(now + 7200, Duration::from_secs(3600)).await,
        1,
        "через час сирота уходит"
    );
    assert_eq!(
        files_in(&root),
        vec![avatar.key.clone()],
        "файл со ссылкой остаётся"
    );
    avatars.pending(avatar.key.clone(), now).await;
    assert_eq!(
        service.sweep(now + 7200, Duration::from_secs(3600)).await,
        0,
        "файл со ссылкой уборка не трогает, даже если о нём осталась запись"
    );
    assert_eq!(service.file(&avatar.key).await.expect("файл"), picture);
    assert!(
        avatars.stale(now + 7200).await.is_empty(),
        "записи разобраны"
    );
}

struct SystemClock;

impl SystemClock {
    fn secs() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_secs())
    }
}
