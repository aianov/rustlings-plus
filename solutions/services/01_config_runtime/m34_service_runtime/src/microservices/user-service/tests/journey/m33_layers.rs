use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use contracts::{UserId, Username};
use journal::{Journal, JournalError};
use proto::social::v1::users_server::Users;
use proto::social::v1::{CreateProfileRequest, FindProfileRequest};
use service_runtime::Health;
use tonic::Request;
use user_service::bootstrap::build;
use user_service::repository::{BoxFuture, RepoError, UserRepository};
use user_service::{Config, MemoryAvatars, MemoryUsers, Relations, User, UserError, UserService};

fn service(users: impl UserRepository + 'static) -> Arc<UserService> {
    Arc::new(UserService::new(
        Arc::new(users),
        Arc::new(MemoryAvatars::default()),
    ))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn one_nickname_cannot_be_taken_twice() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("m33-users-race");
    let _ = fs::remove_dir_all(&dir);
    let file = dir.join("users.ndjson");
    let (journal, entries) = Journal::open(Some(&file)).await.expect("журнал");
    let users = MemoryUsers::restore(journal, entries).expect("репозиторий");
    let service = service(users);
    let mut tasks = Vec::new();
    for _ in 0..32 {
        let service = Arc::clone(&service);
        tasks.push(tokio::spawn(async move { service.register("Alice").await }));
    }
    let mut won = 0;
    for task in tasks {
        match task.await.expect("задача") {
            Ok(user) => {
                won += 1;
                assert_eq!(user.name.as_ref(), "alice");
            }
            Err(UserError::NameTaken(name)) => assert_eq!(name, "alice"),
            Err(other) => panic!("ждали NameTaken, а пришло {other}"),
        }
    }
    assert_eq!(
        won, 1,
        "проверка и запись ника это одна операция репозитория create_if_absent: из 32 одновременных регистраций проходит одна"
    );
    let lines = fs::read_to_string(&file).expect("журнал на диске");
    assert_eq!(
        lines.lines().count(),
        1,
        "в журнале одна регистрация:\n{lines}"
    );
}

#[tokio::test]
async fn social_rules_live_in_the_service() {
    let service = service(MemoryUsers::new());
    let alice = service.register("alice").await.expect("alice").id;
    let bob = service.register("bob").await.expect("bob").id;
    assert!(matches!(
        service.follow(alice, alice).await,
        Err(UserError::SelfFollow)
    ));
    assert!(matches!(
        service.mute(bob, bob).await,
        Err(UserError::SelfMute)
    ));
    assert!(matches!(
        service.follow(alice, UserId(99)).await,
        Err(UserError::UnknownUser(UserId(99)))
    ));
    let bad = service.register("a b").await.expect_err("кривой ник");
    assert_eq!(bad.code(), "bad_data", "{bad}");
    assert!(service.follow(bob, alice).await.expect("подписка"));
    assert!(
        !service.follow(bob, alice).await.expect("повтор"),
        "повтор ничего не меняет"
    );
    let relations = service.relations(alice).await.expect("связи");
    assert_eq!(relations.followers, [bob]);
    let found = service.find("ALICE").await.expect("поиск без регистра");
    assert_eq!(found.id, alice);
    let missing = service.find("nobody").await.expect_err("нет такого");
    assert_eq!(missing.code(), "not_found");
}

struct Broken {
    calls: AtomicUsize,
}

impl UserRepository for Broken {
    fn create_if_absent(&self, _: Username) -> BoxFuture<'_, Result<User, RepoError>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async {
            Err(RepoError::Storage(JournalError::Broken {
                line: 1,
                reason: String::from("диск кончился"),
            }))
        })
    }

    fn get(&self, _: UserId) -> BoxFuture<'_, Option<User>> {
        Box::pin(async { None })
    }

    fn find(&self, _: Username) -> BoxFuture<'_, Option<User>> {
        Box::pin(async { None })
    }

    fn list_after(&self, _: u64, _: usize) -> BoxFuture<'_, Vec<User>> {
        Box::pin(async { Vec::new() })
    }

    fn follow(&self, _: UserId, _: UserId) -> BoxFuture<'_, Result<bool, JournalError>> {
        Box::pin(async { Ok(false) })
    }

    fn mute(&self, _: UserId, _: UserId) -> BoxFuture<'_, Result<bool, JournalError>> {
        Box::pin(async { Ok(false) })
    }

    fn relations(&self, _: UserId) -> BoxFuture<'_, Relations> {
        Box::pin(async { Relations::default() })
    }
}

#[tokio::test]
async fn service_talks_to_storage_only_through_the_trait() {
    let service = service(Broken {
        calls: AtomicUsize::new(0),
    });
    let refused = service
        .register("alice")
        .await
        .expect_err("хранилище сломано");
    assert_eq!(
        refused.code(),
        "internal",
        "сбой хранилища это internal, подробности только в лог: {refused}"
    );
    let early = service.register("").await.expect_err("пустой ник");
    assert_eq!(
        early.code(),
        "bad_data",
        "дешёвая проверка ника стоит до похода в хранилище"
    );
}

#[tokio::test]
async fn every_bootstrap_builds_its_own_store() {
    let first = build(&Config::default(), &Health::default())
        .await
        .expect("первая сборка");
    let second = build(&Config::default(), &Health::default())
        .await
        .expect("вторая сборка");
    let ask = || {
        Request::new(CreateProfileRequest {
            username: "alice".to_string(),
        })
    };
    first.create_profile(ask()).await.expect("alice в первой");
    second
        .create_profile(ask())
        .await
        .expect("и во второй: хранилища не общие, глобального static нет");
    let find = || {
        Request::new(FindProfileRequest {
            username: "alice".to_string(),
        })
    };
    assert_eq!(
        first
            .find_profile(find())
            .await
            .expect("найдена")
            .into_inner()
            .id,
        1
    );
    assert_eq!(
        second
            .find_profile(find())
            .await
            .expect("найдена")
            .into_inner()
            .id,
        1
    );
}
