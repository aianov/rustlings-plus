use std::path::{Path, PathBuf};
use std::sync::Arc;

use contracts::{Origin, UserId, Username};
use e2e::FlakyFiles;
use files::{FileStore, FsFileStore, chunks_of};
use test_support::{Fault, Faults, unique_name, unique_root};
use user_service::{MemoryAvatars, MemoryUsers, UserError, UserService};

fn png(size: usize) -> Vec<u8> {
    let mut data = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR\0\0\0\x20\0\0\0\x20\x08\x06\0\0\0".to_vec();
    let head = data.len();
    data.extend((head..size.max(head)).map(|i| (i % 251) as u8));
    data
}

fn leftovers(root: &Path) -> usize {
    let mut found = 0;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                stack.push(entry.path());
            } else {
                found += 1;
            }
        }
    }
    found
}

async fn service(puts: Faults, reads: Faults) -> (UserService, UserId, PathBuf) {
    let root = std::env::temp_dir()
        .join("e2e-faults")
        .join(unique_root("f"));
    let disk: Arc<dyn FileStore> = Arc::new(FsFileStore::new(&root));
    let flaky = FlakyFiles::new(disk, puts, reads);
    let service = UserService::new(
        Arc::new(MemoryUsers::new()),
        Arc::new(MemoryAvatars::default()),
    )
    .with_files(Arc::new(flaky));
    let name = Username::try_from(unique_name("eve").as_str()).expect("ник");
    let owner = service
        .register(name, &Origin::new("e2e"))
        .await
        .expect("пользователь")
        .id;
    (service, owner, root)
}

#[tokio::test]
async fn a_failed_write_leaves_no_avatar_and_no_file() {
    let puts = Faults::new(Fault::Next(1));
    let (service, owner, root) = service(puts.clone(), Faults::default()).await;
    let picture = png(50_000);
    let failed = service
        .put_avatar(owner, chunks_of(picture.clone(), 4096), None)
        .await;
    assert!(
        matches!(failed, Err(UserError::Files(..))),
        "отказ диска это ошибка загрузки: {failed:?}"
    );
    assert!(service.avatar_of(owner).await.is_none(), "ссылки нет");
    assert_eq!(leftovers(&root), 0, "на диске пусто");
    let (avatar, created) = service
        .put_avatar(owner, chunks_of(picture.clone(), 4096), None)
        .await
        .expect("после отказа повтор проходит");
    assert!(created);
    assert_eq!(service.file(&avatar.key).await.expect("файл"), picture);
    assert_eq!(puts.failed(), 1);
}

#[tokio::test]
async fn a_failed_rename_is_cleaned_up_too() {
    let puts = Faults::calls_failing(&[2]);
    let (service, owner, root) = service(puts, Faults::default()).await;
    let failed = service
        .put_avatar(owner, chunks_of(png(10_000), 1024), None)
        .await;
    assert!(failed.is_err(), "rename не удался: {failed:?}");
    assert!(service.avatar_of(owner).await.is_none());
    assert_eq!(
        service
            .sweep(u64::MAX / 2, std::time::Duration::from_secs(1))
            .await,
        1,
        "временный файл остался сиротой, и уборка его находит"
    );
    assert_eq!(leftovers(&root), 0, "после уборки на диске пусто");
}

#[tokio::test]
async fn a_failed_read_is_an_error_not_a_missing_file() {
    let reads = Faults::new(Fault::Next(1));
    let (service, owner, _) = service(Faults::default(), reads).await;
    let (avatar, _) = service
        .put_avatar(owner, chunks_of(png(3_000), 1024), None)
        .await
        .expect("аватар");
    let failed = service.file(&avatar.key).await;
    assert!(
        matches!(failed, Err(UserError::Files(..))),
        "сбой чтения не выдаётся за «файла нет»: {failed:?}"
    );
    assert!(
        service.file(&avatar.key).await.is_ok(),
        "следующее чтение проходит"
    );
}
