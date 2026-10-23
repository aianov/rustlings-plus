use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use auth_service::repository::BoxFuture;
use auth_service::service::ProfileError;
use auth_service::{
    Account, AuthParts, AuthService, BusRevoker, Mailbox, ProfileDirectory, Registration, Revoker,
};
use contracts::{Email, SessionRevoked, SystemClock, Topics, UserId, Username};
use futures::StreamExt;
use nats_engine::{Bus, BusConfig, DEFAULT_NATS_URL};
use service_runtime::Secret;

type Heard = Arc<Mutex<Vec<(u64, Vec<String>)>>>;

struct Ears(Heard);

impl Revoker for Ears {
    fn revoked(&self, user_id: u64, sessions: Vec<String>) -> BoxFuture<'_, ()> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((user_id, sessions));
        Box::pin(async {})
    }
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

fn secret(raw: &str) -> Secret<String> {
    Secret::new(raw.to_string())
}

fn name(raw: &str) -> Username {
    Username::try_from(raw).expect("ник")
}

fn heard(ears: &Heard) -> Vec<(u64, Vec<String>)> {
    std::mem::take(&mut *ears.lock().unwrap_or_else(PoisonError::into_inner))
}

async fn bench() -> (AuthService, Heard) {
    let ears: Heard = Arc::default();
    let mut parts =
        AuthParts::memory(Arc::new(SystemClock), Arc::new(Mailbox::default())).expect("сервис");
    parts.revoker = Arc::new(Ears(Arc::clone(&ears)));
    let auth = AuthService::new(parts);
    let registration = Registration {
        username: name("alice"),
        email: Some(Email::try_from("alice@example.com").expect("почта")),
        password: secret("correct-horse"),
    };
    auth.register(&Profiles(AtomicU64::new(0)), &registration)
        .await
        .expect("регистрация");
    (auth, ears)
}

#[tokio::test]
async fn a_token_names_its_session_and_a_refresh_keeps_it() {
    let (auth, _) = bench().await;
    let phone = auth
        .login(&name("alice"), &secret("correct-horse"))
        .await
        .expect("вход");
    let laptop = auth
        .login(&name("alice"), &secret("correct-horse"))
        .await
        .expect("второй вход");
    let first = auth.validate(&phone.access).await.expect("токен");
    let second = auth.validate(&laptop.access).await.expect("токен");
    assert!(!first.session.is_empty(), "в токене есть сессия");
    assert_ne!(first.session, second.session, "каждый вход это своя сессия");
    let renewed = auth.refresh(&phone.refresh).await.expect("обновление");
    let renewed = auth.validate(&renewed.access).await.expect("новый токен");
    assert_eq!(
        renewed.session, first.session,
        "обновление по refresh остаётся в той же сессии"
    );
    assert_ne!(renewed.jti, first.jti, "а токен новый");
}

#[tokio::test]
async fn every_way_out_announces_the_closed_sessions() {
    let (auth, ears) = bench().await;
    let (alice, password) = (name("alice"), secret("correct-horse"));
    let login = || auth.login(&alice, &password);
    let phone = login().await.expect("вход");
    let laptop = login().await.expect("вход");
    let tablet = login().await.expect("вход");
    let session = |access: String| {
        let auth = &auth;
        async move { auth.validate(&access).await.expect("токен").session }
    };
    let (phone_session, laptop_session, tablet_session) = (
        session(phone.access.clone()).await,
        session(laptop.access.clone()).await,
        session(tablet.access.clone()).await,
    );
    auth.logout(&phone.access).await.expect("выход");
    assert_eq!(
        heard(&ears),
        [(1, vec![phone_session.clone()])],
        "выход объявляет свою сессию: шлюзы закроют её соединения"
    );
    let rotated = auth.refresh(&laptop.refresh).await.expect("обновление");
    let reused = auth.refresh(&laptop.refresh).await;
    assert!(reused.is_err(), "повтор старого refresh это кража");
    assert_eq!(
        heard(&ears),
        [(1, vec![laptop_session.clone()])],
        "повтор refresh гасит сессию целиком и объявляет её"
    );
    assert!(auth.validate(&rotated.access).await.is_err());
    let closed = auth.logout_all(&tablet.access).await.expect("выход везде");
    assert_eq!(closed, 3, "счёт сессий прежний");
    let all = heard(&ears);
    assert!(
        matches!(all.as_slice(), [(1, sessions)] if sessions.contains(&tablet_session)
            && sessions.contains(&phone_session)
            && sessions.contains(&laptop_session)),
        "выход везде объявляет все сессии пользователя одним сообщением: {all:?}"
    );
}

fn url() -> String {
    std::env::var("RUSTLINGS_NATS_URL")
        .ok()
        .filter(|url| !url.is_empty())
        .unwrap_or_else(|| DEFAULT_NATS_URL.to_string())
}

fn topics() -> Topics {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.subsec_nanos())
        .unwrap_or(0);
    Topics::new(&format!(
        "a{}_{nanos:x}_{}",
        std::process::id() % 100_000,
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}

#[tokio::test]
async fn revocations_travel_through_the_broker() {
    let topics = topics();
    let client = async_nats::connect(url())
        .await
        .expect("NATS курса запущен?");
    let mut gateways = client
        .subscribe(topics.revoked().join("."))
        .await
        .expect("подписка на отзывы");
    client.flush().await.expect("flush");
    let bus = Bus::connect(BusConfig::new(url()))
        .await
        .expect("NATS курса");
    let revoker = BusRevoker::new(bus, topics);
    revoker
        .revoked(7, vec![String::from("s-1"), String::from("s-2")])
        .await;
    let message = tokio::time::timeout(Duration::from_secs(5), gateways.next())
        .await
        .expect("отзыв дошёл")
        .expect("подписка жива");
    let note: SessionRevoked = serde_json::from_slice(&message.payload).expect("тело отзыва");
    assert_eq!(
        note,
        SessionRevoked {
            user: UserId(7),
            sessions: vec![String::from("s-1"), String::from("s-2")],
        },
        "в теле пользователь и его отозванные сессии"
    );
}
