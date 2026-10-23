use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use contracts::{Fact, Origin, UserDeleted, Username};
use journal::Journal;
use user_service::repository::OutboxRepository;
use user_service::{Fate, MemoryAvatars, MemoryUsers, UserError, UserService};

fn name(raw: &str) -> Username {
    Username::try_from(raw).expect("ник")
}

fn journal_path(test: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("m42")
        .join(test);
    let _ = fs::remove_dir_all(&dir);
    dir.join("users.ndjson")
}

async fn open(path: &Path) -> (Arc<MemoryUsers>, UserService) {
    let (journal, lines) = Journal::open(Some(path)).await.expect("журнал");
    let users = Arc::new(MemoryUsers::restore(journal, lines).expect("репозиторий"));
    let service = UserService::new(users.clone(), Arc::new(MemoryAvatars::default()))
        .with_lifecycle(users.clone());
    (users, service)
}

#[tokio::test]
async fn a_hidden_profile_is_gone_until_shown_and_an_erased_one_for_good() {
    let path = journal_path("lifecycle");
    let origin = Origin::new("m42-users");
    let (users, service) = open(&path).await;
    let alice = service
        .register(name("alice"), &origin)
        .await
        .expect("alice")
        .id;
    let bob = service
        .register(name("bob"), &origin)
        .await
        .expect("bob")
        .id;
    let carol = service
        .register(name("carol"), &origin)
        .await
        .expect("carol")
        .id;
    service.follow(bob, alice, &origin).await.expect("подписка");
    service
        .follow(alice, carol, &origin)
        .await
        .expect("подписка");
    assert_eq!(
        service.settle(alice, Fate::Hide, &origin).await.ok(),
        Some(true)
    );
    assert_eq!(
        service.settle(alice, Fate::Hide, &origin).await.ok(),
        Some(false),
        "повтор шага саги не ошибка и ничего не меняет"
    );
    assert!(matches!(
        service.profile(alice).await,
        Err(UserError::UnknownUser(..))
    ));
    assert!(matches!(
        service.find("alice").await,
        Err(UserError::UnknownName(..))
    ));
    assert!(
        matches!(
            service.follow(carol, alice, &origin).await,
            Err(UserError::UnknownUser(..))
        ),
        "на скрытый профиль не подписаться"
    );
    assert!(
        service
            .list(0, 10)
            .await
            .iter()
            .all(|user| user.id != alice),
        "скрытый профиль не виден в списке"
    );
    assert_eq!(
        service.settle(alice, Fate::Show, &origin).await.ok(),
        Some(true)
    );
    assert_eq!(
        service
            .profile(alice)
            .await
            .expect("вернулся")
            .name
            .as_ref(),
        "alice",
        "компенсация возвращает профиль как был"
    );
    assert_eq!(
        service.settle(alice, Fate::Erase, &origin).await.ok(),
        Some(true)
    );
    assert_eq!(
        service.settle(alice, Fate::Erase, &origin).await.ok(),
        Some(false)
    );
    let rows = users.unpublished(100).await;
    assert!(
        rows.iter()
            .any(|row| row.event.fact()
                == Ok(Some(Fact::UserDeleted(UserDeleted { user_id: alice })))),
        "удаление это событие user.deleted в outbox: по нему лента забудет автора"
    );
    assert!(
        !rows
            .iter()
            .any(|row| matches!(row.event.kind.as_str(), "user.hidden" | "user.shown")),
        "скрытие внутреннее дело саги, событий о нём нет"
    );
    let relations = service.relations(bob).await.expect("bob");
    assert!(
        relations.following.is_empty(),
        "подписки на стёртый профиль нет"
    );
    drop(service);
    drop(users);
    let (_, service) = open(&path).await;
    assert!(
        matches!(
            service.profile(alice).await,
            Err(UserError::UnknownUser(..))
        ),
        "после перезапуска стёртый профиль не воскресает"
    );
    assert!(
        service
            .relations(carol)
            .await
            .expect("carol")
            .followers
            .is_empty()
    );
    let again = service
        .register(name("alice"), &origin)
        .await
        .expect("ник свободен");
    assert_ne!(again.id, alice, "у нового профиля новый id");
}
