use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use auth_service::repository::{BoxFuture, RefreshError};
use auth_service::service::ProfileError;
use auth_service::service::token::hmac::sha256_hex;
use auth_service::{
    Account, AuthError, AuthParts, AuthService, Mailbox, ProfileDirectory, Registration,
};
use contracts::{Clock, Email, ErrorCode, Role, Username};
use service_runtime::Secret;

struct ManualClock(AtomicU64);

impl ManualClock {
    fn advance(&self, seconds: u64) {
        self.0.fetch_add(seconds, Ordering::SeqCst);
    }
}

impl Clock for ManualClock {
    fn now(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
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

struct Bench {
    auth: AuthService,
    clock: Arc<ManualClock>,
    mail: Mailbox,
    profiles: Profiles,
}

fn secret(raw: &str) -> Secret<String> {
    Secret::new(raw.to_string())
}

fn name(raw: &str) -> Username {
    Username::try_from(raw).expect("ник")
}

impl Bench {
    fn new(moderators: &[&str]) -> Bench {
        let clock = Arc::new(ManualClock(AtomicU64::new(1_000_000)));
        let mail = Mailbox::default();
        let mut parts =
            AuthParts::memory(clock.clone(), Arc::new(mail.clone())).expect("сервис в памяти");
        parts.moderators = moderators.iter().map(|name| name.to_string()).collect();
        Bench {
            auth: AuthService::new(parts),
            clock,
            mail,
            profiles: Profiles(AtomicU64::new(0)),
        }
    }

    async fn register(&self, raw: &str, email: Option<&str>) -> Account {
        let registration = Registration {
            username: name(raw),
            email: email.map(|email| Email::try_from(email).expect("почта")),
            password: secret("correct-horse"),
        };
        self.auth
            .register(&self.profiles, &registration)
            .await
            .expect("регистрация")
    }

    async fn login(&self, raw: &str) -> auth_service::Issued {
        self.auth
            .login(&name(raw), &secret("correct-horse"))
            .await
            .expect("вход")
    }

    fn token_from_letter(&self) -> String {
        let letters = self.mail.letters();
        let link = &letters.last().expect("письмо ушло").link;
        link.rsplit_once("token=")
            .expect("ссылка с токеном")
            .1
            .to_string()
    }
}

#[tokio::test]
async fn wrong_password_and_unknown_name_look_the_same() {
    let bench = Bench::new(&[]);
    bench.register("alice", None).await;
    let wrong = bench
        .auth
        .login(&name("alice"), &secret("battery-staple"))
        .await
        .expect_err("неверный пароль");
    let unknown = bench
        .auth
        .login(&name("bobby"), &secret("correct-horse"))
        .await
        .expect_err("нет такого");
    assert_eq!(
        (wrong.code(), wrong.to_string()),
        (unknown.code(), unknown.to_string()),
        "по ответу нельзя узнать, есть ли такой ник"
    );
    assert_eq!(wrong.code(), ErrorCode::Unauthorized);
}

#[tokio::test]
async fn refresh_rotates_and_a_replay_burns_the_whole_family() {
    let bench = Bench::new(&[]);
    bench.register("alice", None).await;
    let laptop = bench.login("alice").await;
    let phone = bench.login("alice").await;
    let next = bench.auth.refresh(&laptop.refresh).await.expect("обмен");
    assert_ne!(
        next.refresh, laptop.refresh,
        "каждый обмен выдаёт новую пару"
    );
    assert!(bench.auth.validate(&next.access).await.is_ok());
    let replay = bench
        .auth
        .refresh(&laptop.refresh)
        .await
        .expect_err("повтор старого refresh");
    assert!(
        matches!(replay, AuthError::Refresh(RefreshError::Reused)),
        "{replay}"
    );
    assert!(
        bench.auth.refresh(&next.refresh).await.is_err(),
        "повтор это сигнал кражи: гаснет и свежий refresh этого входа"
    );
    assert!(
        matches!(
            bench.auth.validate(&next.access).await,
            Err(AuthError::TokenRevoked)
        ),
        "и его access тоже"
    );
    assert!(
        bench.auth.refresh(&phone.refresh).await.is_ok(),
        "другое устройство это другое семейство: его не трогаем"
    );
    let stolen_row = sha256_hex(&phone.refresh);
    assert!(
        bench.auth.refresh(&stolen_row).await.is_err(),
        "в хранилище хеш refresh: строка из дампа не открывает вход"
    );
}

#[tokio::test]
async fn access_expires_with_its_own_code_and_refresh_has_an_exact_end() {
    let bench = Bench::new(&[]);
    bench.register("alice", None).await;
    let session = bench.login("alice").await;
    let lifetimes = bench.auth.lifetimes();
    bench.clock.advance(lifetimes.access + 30);
    let expired = bench
        .auth
        .validate(&session.access)
        .await
        .expect_err("срок вышел");
    assert_eq!(
        expired.code(),
        ErrorCode::Expired,
        "отдельный код: клиент обновит токен, а не выкинет человека на вход"
    );
    let renewed = bench.auth.refresh(&session.refresh).await.expect("обмен");
    assert!(bench.auth.validate(&renewed.access).await.is_ok());
    bench.clock.advance(lifetimes.refresh);
    assert!(
        matches!(
            bench.auth.refresh(&renewed.refresh).await,
            Err(AuthError::Refresh(RefreshError::Expired))
        ),
        "now >= expires: ровно в секунду окончания refresh уже не годится"
    );
}

#[tokio::test]
async fn logout_logout_all_and_password_change_revoke_what_they_should() {
    let bench = Bench::new(&[]);
    bench.register("alice", None).await;
    let laptop = bench.login("alice").await;
    bench.auth.logout(&laptop.access).await.expect("выход");
    assert!(matches!(
        bench.auth.validate(&laptop.access).await,
        Err(AuthError::TokenRevoked)
    ));
    assert!(bench.auth.refresh(&laptop.refresh).await.is_err());
    let phone = bench.login("alice").await;
    let tablet = bench.login("alice").await;
    let closed = bench
        .auth
        .logout_all(&phone.access)
        .await
        .expect("выход везде");
    assert_eq!(
        closed, 3,
        "все семейства пользователя, включая уже закрытое"
    );
    for device in [&phone, &tablet] {
        assert!(bench.auth.validate(&device.access).await.is_err());
        assert!(bench.auth.refresh(&device.refresh).await.is_err());
    }
    let current = bench.login("alice").await;
    let other = bench.login("alice").await;
    bench
        .auth
        .change_password(
            &current.access,
            &secret("correct-horse"),
            &secret("battery-staple"),
        )
        .await
        .expect("смена пароля");
    assert!(
        bench.auth.validate(&current.access).await.is_ok(),
        "смена пароля не выкидывает того, кто её сделал"
    );
    assert!(bench.auth.validate(&other.access).await.is_err());
    assert!(
        bench
            .auth
            .login(&name("alice"), &secret("correct-horse"))
            .await
            .is_err()
    );
    assert!(
        bench
            .auth
            .login(&name("alice"), &secret("battery-staple"))
            .await
            .is_ok()
    );
    bench.clock.advance(bench.auth.lifetimes().refresh + 60);
    assert!(
        bench.auth.purge().await > 0,
        "просроченное чистится, список отзыва не растёт вечно"
    );
}

#[tokio::test]
async fn one_time_links_are_single_use_bound_and_short_lived() {
    let bench = Bench::new(&[]);
    bench.register("alice", Some("alice@example.com")).await;
    let session = bench.login("alice").await;
    bench
        .auth
        .request_confirmation(&session.access)
        .await
        .expect("письмо");
    let first = bench.token_from_letter();
    bench
        .auth
        .request_confirmation(&session.access)
        .await
        .expect("ещё письмо");
    let second = bench.token_from_letter();
    assert!(
        bench.auth.confirm_email(&first).await.is_err(),
        "новая ссылка отменяет прежнюю"
    );
    bench
        .auth
        .forgot_password(&name("alice"))
        .await
        .expect("сброс");
    let reset = bench.token_from_letter();
    assert!(
        matches!(
            bench.auth.confirm_email(&reset).await,
            Err(AuthError::OnceInvalid)
        ),
        "ссылка сброса не подтверждает почту"
    );
    bench
        .auth
        .confirm_email(&second)
        .await
        .expect("подтверждение");
    assert!(
        bench.auth.confirm_email(&second).await.is_err(),
        "ссылка сгорает при использовании"
    );
    assert!(bench.auth.account(1).await.expect("аккаунт").email_verified);
    bench.clock.advance(bench.auth.lifetimes().reset);
    assert!(
        bench
            .auth
            .reset_password(&reset, &secret("new-horse-1"))
            .await
            .is_err(),
        "ссылка сброса живёт час, и ровно через час уже нет"
    );
    bench
        .auth
        .forgot_password(&name("alice"))
        .await
        .expect("сброс");
    let fresh = bench.token_from_letter();
    bench
        .auth
        .reset_password(&fresh, &secret("new-horse-1"))
        .await
        .expect("новый пароль");
    assert!(
        bench.auth.validate(&session.access).await.is_err(),
        "после сброса пароля все сессии закрыты"
    );
    let before = bench.mail.letters().len();
    bench
        .auth
        .forgot_password(&name("ghost"))
        .await
        .expect("молча");
    assert_eq!(
        bench.mail.letters().len(),
        before,
        "на чужой ник письмо не уходит, а ответ тот же"
    );
}

#[tokio::test]
async fn roles_come_from_the_account_and_ride_in_the_token() {
    let bench = Bench::new(&["moder"]);
    bench.register("moder", None).await;
    bench.register("alice", None).await;
    let moder = bench
        .auth
        .validate(&bench.login("moder").await.access)
        .await
        .expect("токен");
    let alice = bench
        .auth
        .validate(&bench.login("alice").await.access)
        .await
        .expect("токен");
    assert_eq!((moder.role, alice.role), (Role::Moderator, Role::User));
}
