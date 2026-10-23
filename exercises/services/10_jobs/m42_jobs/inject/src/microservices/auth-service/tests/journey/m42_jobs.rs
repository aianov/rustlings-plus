use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use auth_service::repository::BoxFuture;
use auth_service::service::{Deletion, DeletionQueue, PostEraser, ProfileError, ProfileKeeper};
use auth_service::{Account, AuthParts, AuthService, Mailbox, ProfileDirectory, Registration};
use contracts::{Email, SystemClock, Username};
use jobs::{Checkpoints, Failure, SagaError, SagaLog};
use nats_engine::{Bus, BusConfig, DEFAULT_NATS_URL};
use service_runtime::Secret;

type Trail = Arc<Mutex<Vec<String>>>;

fn note(trail: &Trail, line: String) {
    trail
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(line);
}

fn seen(trail: &Trail) -> Vec<String> {
    trail.lock().unwrap_or_else(PoisonError::into_inner).clone()
}

struct Profiles(AtomicU64);

impl ProfileDirectory for Profiles {
    fn create(&self, username: String) -> BoxFuture<'_, Result<Account, ProfileError>> {
        Box::pin(async move {
            let id = self.0.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(Account::new(id, &username))
        })
    }
}

struct Keeper {
    trail: Trail,
    refuse_hide: bool,
}

impl ProfileKeeper for Keeper {
    fn hide(&self, user_id: u64) -> BoxFuture<'_, Result<(), Failure>> {
        Box::pin(async move {
            note(&self.trail, format!("hide {user_id}"));
            match self.refuse_hide {
                true => Err(Failure::Permanent(String::from("профиль не скрыть"))),
                false => Ok(()),
            }
        })
    }

    fn show(&self, user_id: u64) -> BoxFuture<'_, Result<(), Failure>> {
        Box::pin(async move {
            note(&self.trail, format!("show {user_id}"));
            Ok(())
        })
    }

    fn erase(&self, user_id: u64) -> BoxFuture<'_, Result<(), Failure>> {
        Box::pin(async move {
            note(&self.trail, format!("erase {user_id}"));
            Ok(())
        })
    }
}

struct Posts {
    trail: Trail,
    left: Mutex<Vec<u64>>,
    stumble_on: AtomicU64,
    stumbled: AtomicBool,
}

impl PostEraser for Posts {
    fn page(&self, _: u64, limit: u32) -> BoxFuture<'_, Result<Vec<u64>, Failure>> {
        Box::pin(async move {
            let left = self.left.lock().unwrap_or_else(PoisonError::into_inner);
            Ok(left.iter().copied().take(limit as usize).collect())
        })
    }

    fn delete(&self, _: u64, post_id: u64) -> BoxFuture<'_, Result<(), Failure>> {
        Box::pin(async move {
            if post_id == self.stumble_on.load(Ordering::SeqCst)
                && !self.stumbled.swap(true, Ordering::SeqCst)
            {
                return Err(Failure::Transient(String::from("post-service моргнул")));
            }
            note(&self.trail, format!("post {post_id}"));
            self.left
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .retain(|left| *left != post_id);
            Ok(())
        })
    }
}

#[derive(Default)]
struct Queued(Mutex<Vec<(String, u64)>>);

impl DeletionQueue for Queued {
    fn request(&self, saga: String, user_id: u64) -> BoxFuture<'_, Result<(), Failure>> {
        Box::pin(async move {
            self.0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push((saga, user_id));
            Ok(())
        })
    }
}

fn url() -> String {
    std::env::var("RUSTLINGS_NATS_URL")
        .ok()
        .filter(|url| !url.is_empty())
        .unwrap_or_else(|| DEFAULT_NATS_URL.to_string())
}

fn bucket(name: &str) -> String {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.subsec_nanos())
        .unwrap_or(0);
    format!(
        "D{}_{nanos:X}_{}_{name}",
        std::process::id() % 100_000,
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

struct Bench {
    auth: AuthService,
    deletion: Deletion,
    trail: Trail,
    buckets: Vec<String>,
}

fn secret(raw: &str) -> Secret<String> {
    Secret::new(raw.to_string())
}

fn name(raw: &str) -> Username {
    Username::try_from(raw).expect("ник")
}

impl Bench {
    async fn open(refuse_hide: bool, posts: Vec<u64>, stumble_on: u64) -> Bench {
        let parts =
            AuthParts::memory(Arc::new(SystemClock), Arc::new(Mailbox::default())).expect("сервис");
        let auth = AuthService::new(parts);
        let registration = Registration {
            username: name("alice"),
            email: Some(Email::try_from("alice@example.com").expect("почта")),
            password: secret("correct-horse"),
        };
        auth.register(&Profiles(AtomicU64::new(0)), &registration)
            .await
            .expect("регистрация");
        let bus = Bus::connect(BusConfig::new(url()))
            .await
            .expect("NATS курса");
        let (sagas, points) = (bucket("SAGAS"), bucket("CHECKPOINTS"));
        let trail: Trail = Arc::default();
        let deletion = Deletion {
            profiles: Arc::new(Keeper {
                trail: Arc::clone(&trail),
                refuse_hide,
            }),
            posts: Arc::new(Posts {
                trail: Arc::clone(&trail),
                left: Mutex::new(posts),
                stumble_on: AtomicU64::new(stumble_on),
                stumbled: AtomicBool::new(false),
            }),
            log: SagaLog::open(&bus, &sagas).await.expect("журнал саги"),
            checkpoints: Checkpoints::open(&bus, &points).await.expect("точки"),
        };
        Bench {
            auth,
            deletion,
            trail,
            buckets: vec![sagas, points],
        }
    }

    async fn login(&self) -> bool {
        self.auth
            .login(&name("alice"), &secret("correct-horse"))
            .await
            .is_ok()
    }

    async fn close(self) {
        let js = async_nats::jetstream::new(async_nats::connect(url()).await.expect("NATS"));
        for bucket in self.buckets {
            let _ = js.delete_stream(format!("KV_{bucket}")).await;
        }
    }
}

#[tokio::test]
async fn deletion_goes_auth_user_posts_and_resumes_after_a_stumble() {
    let bench = Bench::open(false, vec![3, 5, 8], 5).await;
    let issued = bench
        .auth
        .login(&name("alice"), &secret("correct-horse"))
        .await
        .expect("вход");
    let queue = Queued::default();
    let saga = bench
        .auth
        .request_deletion(&issued.access, &queue)
        .await
        .expect("удаление поставлено");
    assert_eq!(
        *queue.0.lock().unwrap_or_else(PoisonError::into_inner),
        [(saga.clone(), 1)],
        "запрос только ставит задачу: сага идёт в воркере, а ответ приходит сразу"
    );
    let first = bench.auth.delete_account(&saga, 1, &bench.deletion).await;
    assert!(
        matches!(first, Err(SagaError::Retry(..))),
        "post-service моргнул посреди постов: после необратимого шага сага ждёт повтора, а не откатывается: {first:?}"
    );
    assert!(!bench.login().await, "аккаунт заморожен: войти нельзя");
    assert!(
        bench.auth.validate(&issued.access).await.is_err(),
        "заморозка гасит выданные токены"
    );
    let second = bench.auth.delete_account(&saga, 1, &bench.deletion).await;
    assert_eq!(second, Ok(()), "повтор задачи доделал сагу");
    assert_eq!(
        seen(&bench.trail),
        ["hide 1", "post 3", "post 5", "post 8", "erase 1"],
        "порядок auth, user, посты с контрольной точкой (пост 3 не удалялся дважды), профиль"
    );
    assert!(!bench.login().await, "учётной записи больше нет");
    bench.close().await;
}

#[tokio::test]
async fn a_refused_step_rolls_the_account_back() {
    let bench = Bench::open(true, vec![1], 0).await;
    let failed = bench
        .auth
        .delete_account("delete.1.9", 1, &bench.deletion)
        .await;
    assert!(
        matches!(&failed, Err(SagaError::RolledBack { failed, stuck, .. }) if failed == "hide" && stuck.is_empty()),
        "{failed:?}"
    );
    assert_eq!(
        seen(&bench.trail),
        ["hide 1"],
        "упавший шаг не откатывается, постов никто не трогал"
    );
    assert!(
        bench.login().await,
        "заморозка откачена: alice снова входит"
    );
    bench.close().await;
}
