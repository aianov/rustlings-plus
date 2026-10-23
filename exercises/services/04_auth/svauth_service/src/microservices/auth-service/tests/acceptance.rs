use argon2::Params;
use auth_service::bootstrap::build;
use auth_service::common::{HttpReply, HttpRequest};
use auth_service::controller::AuthApp;
use auth_service::service::{Config, ValidateError};
use authkit::{KeySet, ManualClock};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

const START: u64 = 1_700_000_000;

struct Probe {
    calls: AtomicUsize,
    now: AtomicUsize,
    peak: AtomicUsize,
    pause_ms: AtomicUsize,
}

fn app_with(permits: usize) -> (Arc<AuthApp>, Arc<ManualClock>, Arc<Probe>) {
    let clock = Arc::new(ManualClock::new(START));
    let probe = Arc::new(Probe { calls: AtomicUsize::new(0), now: AtomicUsize::new(0), peak: AtomicUsize::new(0), pause_ms: AtomicUsize::new(0) });
    let hook = probe.clone();
    let config = Config {
        clock: clock.clone(),
        keys: KeySet::new("k1", b"secret-one"),
        params: Params::new(8, 1, 1, None).unwrap(),
        max_parallel: permits,
        probe: Arc::new(move || {
            hook.calls.fetch_add(1, Ordering::SeqCst);
            let current = hook.now.fetch_add(1, Ordering::SeqCst) + 1;
            hook.peak.fetch_max(current, Ordering::SeqCst);
            let pause = hook.pause_ms.load(Ordering::SeqCst);
            if pause > 0 {
                std::thread::sleep(Duration::from_millis(pause as u64));
            }
            hook.now.fetch_sub(1, Ordering::SeqCst);
        }),
    };
    (Arc::new(build(config)), clock, probe)
}

fn app() -> (Arc<AuthApp>, Arc<ManualClock>, Arc<Probe>) {
    app_with(2)
}

fn post(path: &str, body: &str, bearer: Option<&str>) -> HttpRequest {
    let mut headers = vec![("Content-Type".to_string(), "application/json".to_string())];
    if let Some(token) = bearer {
        headers.push(("Authorization".to_string(), format!("Bearer {token}")));
    }
    HttpRequest { method: "POST".to_string(), path: path.to_string(), headers, body: body.as_bytes().to_vec() }
}

async fn register(app: &AuthApp, name: &str, password: &str) -> HttpReply {
    let body = format!(r#"{{"username":"{name}","email":"{name}@example.com","password":"{password}"}}"#);
    app.handle(post("/register", &body, None)).await
}

async fn login(app: &AuthApp, name: &str, password: &str) -> HttpReply {
    app.handle(post("/login", &format!(r#"{{"username":"{name}","password":"{password}"}}"#), None)).await
}

fn field(reply: &HttpReply, name: &str) -> String {
    reply.json()[name].as_str().unwrap_or_else(|| panic!("в ответе нет строки {name}: {}", reply.body)).to_string()
}

fn code(reply: &HttpReply) -> String {
    reply.json()["error"]["code"].as_str().unwrap_or("<нет кода>").to_string()
}

async fn signed_up(app: &AuthApp, name: &str) -> HttpReply {
    let reply = register(app, name, "correct horse").await;
    assert_eq!(reply.status, 201, "регистрация {name}: {}", reply.body);
    login(app, name, "correct horse").await
}

#[tokio::test]
async fn register_login_validate_roundtrip() {
    let (app, _, _) = app();
    let created = register(&app, "alice", "correct horse").await;
    assert_eq!(created.status, 201, "регистрация: {}", created.body);
    let id = field(&created, "id");
    let tokens = login(&app, "alice", "correct horse").await;
    assert_eq!(tokens.status, 200, "вход: {}", tokens.body);
    assert_eq!(tokens.json()["expires_in"], 900, "access живёт 15 минут");
    let principal = app.validate_token(&field(&tokens, "access")).expect("свежий токен проходит ValidateToken");
    assert_eq!((principal.user_id.as_str(), principal.role.as_str()), (id.as_str(), "user"), "в токене id и роль");
    let account = app.get_account(&id).expect("GetAccount");
    assert_eq!((account.username.as_str(), account.email.as_str()), ("alice", "alice@example.com"), "данные аккаунта");
    assert!(!format!("{account:?}").contains("argon2"), "аккаунт для других сервисов не несёт хеш пароля");
    assert!(app.get_account("u-999").is_none(), "несуществующий аккаунт");
}

#[tokio::test]
async fn passwords_never_stored_or_echoed_in_the_clear() {
    let (app, _, _) = app();
    let created = register(&app, "alice", "correct horse").await;
    let tokens = login(&app, "alice", "correct horse").await;
    for reply in [&created, &tokens] {
        assert!(!reply.body.contains("correct horse"), "пароль вернулся в ответе: {}", reply.body);
    }
    let hashes = app.service().store().password_hashes();
    assert_eq!(hashes.len(), 1, "один пользователь");
    assert!(hashes[0].starts_with("$argon2id$") && !hashes[0].contains("correct horse"), "пароль хранится как argon2id: {}", hashes[0]);
    let refresh = field(&tokens, "refresh");
    assert!(app.service().store().session_keys().iter().all(|k| *k != refresh), "refresh в таблице сессий лежит хешем");
}

#[tokio::test]
async fn bad_input_is_told_apart() {
    let (app, _, _) = app();
    let reply = app.handle(post("/register", "{", None)).await;
    assert_eq!((reply.status, code(&reply).as_str()), (400, "request.malformed"), "битый JSON: {}", reply.body);
    let reply = app.handle(post("/register", "[1,2]", None)).await;
    assert_eq!(reply.status, 400, "не объект");
    let reply = app.handle(post("/register", r#"{"username":"a","email":"nope","password":"123"}"#, None)).await;
    assert_eq!((reply.status, code(&reply).as_str()), (422, "validation.failed"), "поля не годятся: {}", reply.body);
    let details: Vec<(String, String)> = reply.json()["error"]["details"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| (d["field"].as_str().unwrap().to_string(), d["code"].as_str().unwrap().to_string()))
        .collect();
    let expected = [("username", "too_short"), ("email", "invalid"), ("password", "too_short")];
    assert_eq!(details, expected.iter().map(|(f, c)| (f.to_string(), c.to_string())).collect::<Vec<_>>(), "все нарушения сразу, в порядке полей");
    assert!(!reply.body.contains("123"), "значения полей в ответ не возвращаются");
    let reply = app.handle(post("/register", r#"{"username":"1abc","email":"a@b.io","password":"correct horse"}"#, None)).await;
    assert!(reply.body.contains("invalid"), "ник не может начинаться с цифры: {}", reply.body);
    let reply = app.handle(post("/register", r#"{"email":"a@b.io","password":"correct horse"}"#, None)).await;
    assert!(reply.body.contains("required"), "ника нет: {}", reply.body);
}

#[tokio::test]
async fn password_length_counts_characters() {
    let (app, _, _) = app();
    assert_eq!(register(&app, "olga", "пароль12").await.status, 201, "восемь символов (шестнадцать байт) подходят");
    assert_eq!(register(&app, "ivan", "пароль1").await.status, 422, "семь символов это мало, сколько бы байт они ни занимали");
}

#[tokio::test]
async fn usernames_are_unique_ignoring_case_and_spaces() {
    let (app, _, _) = app();
    assert_eq!(register(&app, "alice", "correct horse").await.status, 201, "первая регистрация");
    let again = app.handle(post("/register", r#"{"username":" Alice ","email":"second@example.com","password":"another pass1"}"#, None)).await;
    assert_eq!((again.status, code(&again).as_str()), (409, "user.username_taken"), "тот же ник другим регистром: {}", again.body);
    let tokens = login(&app, "ALICE", "correct horse").await;
    assert_eq!(tokens.status, 200, "входить можно с любым регистром ника");
}

#[tokio::test]
async fn concurrent_registrations_of_one_name_have_one_winner() {
    let (app, _, _) = app_with(4);
    let handles: Vec<_> = (0..6)
        .map(|i| {
            let app = app.clone();
            tokio::spawn(async move { register(&app, "alice", &format!("password-{i}-long")).await.status })
        })
        .collect();
    let mut statuses = Vec::new();
    for handle in handles {
        statuses.push(handle.await.unwrap());
    }
    assert_eq!(statuses.iter().filter(|s| **s == 201).count(), 1, "создан один аккаунт: {statuses:?}");
    assert_eq!(app.service().store().password_hashes().len(), 1, "и в базе одна запись");
}

#[tokio::test]
async fn login_does_not_reveal_which_part_was_wrong() {
    let (app, _, probe) = app();
    register(&app, "alice", "correct horse").await;
    let before = probe.calls.load(Ordering::SeqCst);
    let wrong = login(&app, "alice", "wrong password").await;
    let after_wrong = probe.calls.load(Ordering::SeqCst);
    let unknown = login(&app, "nobody", "wrong password").await;
    let after_unknown = probe.calls.load(Ordering::SeqCst);
    assert_eq!((wrong.status, code(&wrong).as_str()), (401, "auth.invalid_credentials"), "неверный пароль: {}", wrong.body);
    assert_eq!(wrong, unknown, "ответы для неверного пароля и неизвестного ника должны совпадать до байта: {wrong:?} против {unknown:?}");
    assert_eq!(after_wrong - before, 1, "проверка пароля идёт один раз");
    assert_eq!(after_unknown - after_wrong, 1, "и для неизвестного ника тоже: иначе по времени ответа видно, что такого пользователя нет");
}

#[tokio::test(flavor = "current_thread")]
async fn hashing_is_off_the_runtime_and_bounded() {
    let (app, _, probe) = app_with(2);
    for name in ["u1", "u2", "u3", "u4", "u5", "u6"] {
        assert_eq!(register(&app, &format!("user_{name}"), "correct horse").await.status, 201, "регистрация {name}");
    }
    probe.pause_ms.store(40, Ordering::SeqCst);
    probe.peak.store(0, Ordering::SeqCst);
    let ticks = Arc::new(AtomicUsize::new(0));
    let ticker = {
        let ticks = ticks.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(5));
            loop {
                interval.tick().await;
                ticks.fetch_add(1, Ordering::SeqCst);
            }
        })
    };
    tokio::time::sleep(Duration::from_millis(20)).await;
    let before = ticks.load(Ordering::SeqCst);
    let handles: Vec<_> = ["u1", "u2", "u3", "u4", "u5", "u6"]
        .iter()
        .map(|name| {
            let (app, name) = (app.clone(), format!("user_{name}"));
            tokio::spawn(async move { login(&app, &name, "correct horse").await.status })
        })
        .collect();
    for handle in handles {
        assert_eq!(handle.await.unwrap(), 200, "вход");
    }
    let ticked = ticks.load(Ordering::SeqCst) - before;
    ticker.abort();
    assert!(ticked >= 8, "рантайм почти не тикал ({ticked}) пока считались хеши: argon2 надо выносить в spawn_blocking");
    assert_eq!(probe.peak.load(Ordering::SeqCst), 2, "одновременно считаются ровно две проверки при лимите 2");
}

#[tokio::test]
async fn refresh_rotates_and_reuse_burns_the_family() {
    let (app, _, _) = app();
    let first = signed_up(&app, "alice").await;
    let r1 = field(&first, "refresh");
    let second = app.handle(post("/refresh", &format!(r#"{{"refresh":"{r1}"}}"#), None)).await;
    assert_eq!(second.status, 200, "обмен: {}", second.body);
    let (r2, a2) = (field(&second, "refresh"), field(&second, "access"));
    assert_ne!(r1, r2, "refresh меняется");
    let replay = app.handle(post("/refresh", &format!(r#"{{"refresh":"{r1}"}}"#), None)).await;
    assert_eq!((replay.status, code(&replay).as_str()), (401, "auth.invalid_refresh"), "повтор использованного: {}", replay.body);
    let after = app.handle(post("/refresh", &format!(r#"{{"refresh":"{r2}"}}"#), None)).await;
    assert_eq!(after.status, 401, "свежий refresh того же входа тоже погашен");
    assert_eq!(app.validate_token(&a2), Err(ValidateError::Revoked), "и access этой цепочки отозван");
}

#[tokio::test]
async fn unknown_or_empty_refresh_is_rejected() {
    let (app, _, _) = app();
    let reply = app.handle(post("/refresh", r#"{"refresh":"nope"}"#, None)).await;
    assert_eq!((reply.status, code(&reply).as_str()), (401, "auth.invalid_refresh"), "неизвестный: {}", reply.body);
    let reply = app.handle(post("/refresh", r#"{}"#, None)).await;
    assert_eq!(reply.status, 422, "поле refresh обязательно: {}", reply.body);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn simultaneous_refresh_has_one_winner() {
    let (app, _, _) = app_with(4);
    let tokens = signed_up(&app, "alice").await;
    let refresh = field(&tokens, "refresh");
    let barrier = Arc::new(tokio::sync::Barrier::new(8));
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let (app, barrier, refresh) = (app.clone(), barrier.clone(), refresh.clone());
            tokio::spawn(async move {
                barrier.wait().await;
                app.handle(post("/refresh", &format!(r#"{{"refresh":"{refresh}"}}"#), None)).await.status
            })
        })
        .collect();
    let mut wins = 0;
    for handle in handles {
        if handle.await.unwrap() == 200 {
            wins += 1;
        }
    }
    assert_eq!(wins, 1, "одним refresh обменялись {wins} раз: проверка и пометка должны быть в одной транзакции");
}

#[tokio::test]
async fn logout_kills_access_and_refresh_of_that_session() {
    let (app, _, _) = app();
    let tokens = signed_up(&app, "alice").await;
    let (access, refresh) = (field(&tokens, "access"), field(&tokens, "refresh"));
    let reply = app.handle(post("/logout", "", Some(&access))).await;
    assert_eq!(reply.status, 204, "выход: {}", reply.body);
    assert_eq!(app.validate_token(&access), Err(ValidateError::Revoked), "access, подпись и срок которого в порядке, отозван");
    let reply = app.handle(post("/refresh", &format!(r#"{{"refresh":"{refresh}"}}"#), None)).await;
    assert_eq!(reply.status, 401, "refresh этой сессии погашен");
    let reply = app.handle(post("/logout", "", None)).await;
    assert_eq!((reply.status, code(&reply).as_str()), (401, "auth.unauthenticated"), "без токена: {}", reply.body);
    let reply = app.handle(post("/logout", "", Some("garbage"))).await;
    assert_eq!(reply.status, 401, "с мусором вместо токена");
}

#[tokio::test]
async fn logout_all_is_per_user() {
    let (app, _, _) = app();
    let phone = signed_up(&app, "alice").await;
    let laptop = login(&app, "alice", "correct horse").await;
    let bob_tokens = signed_up(&app, "bobby").await;
    let reply = app.handle(post("/logout-all", "", Some(&field(&phone, "access")))).await;
    assert_eq!(reply.status, 204, "выход везде: {}", reply.body);
    for tokens in [&phone, &laptop] {
        assert_eq!(app.validate_token(&field(tokens, "access")), Err(ValidateError::Revoked), "access alice отозван");
        let reply = app.handle(post("/refresh", &format!(r#"{{"refresh":"{}"}}"#, field(tokens, "refresh")), None)).await;
        assert_eq!(reply.status, 401, "refresh alice погашен");
    }
    assert!(app.validate_token(&field(&bob_tokens, "access")).is_ok(), "bobby не затронут");
}

#[tokio::test]
async fn token_lifetime_and_refresh_lifetime() {
    let (app, clock, _) = app();
    let tokens = signed_up(&app, "alice").await;
    let access = field(&tokens, "access");
    clock.advance(900 + 29);
    assert!(app.validate_token(&access).is_ok(), "допуск на рассинхрон часов 30 секунд");
    clock.advance(1);
    assert_eq!(app.validate_token(&access), Err(ValidateError::Expired), "срок access вышел");
    let reply = app.handle(post("/logout", "", Some(&access))).await;
    assert_eq!((reply.status, code(&reply).as_str()), (401, "auth.expired"), "просроченный токен отличим от неверного: {}", reply.body);
    let renewed = app.handle(post("/refresh", &format!(r#"{{"refresh":"{}"}}"#, field(&tokens, "refresh")), None)).await;
    assert_eq!(renewed.status, 200, "refresh живёт дольше access: {}", renewed.body);
    clock.advance(30 * 24 * 3600);
    let late = app.handle(post("/refresh", &format!(r#"{{"refresh":"{}"}}"#, field(&renewed, "refresh")), None)).await;
    assert_eq!(late.status, 401, "через 30 суток refresh протух");
}

#[tokio::test]
async fn signing_key_rotation_does_not_log_anyone_out() {
    let (app, _, _) = app();
    let old = field(&signed_up(&app, "alice").await, "access");
    app.service().rotate_key("k2", b"secret-two");
    assert!(app.validate_token(&old).is_ok(), "токены прошлого ключа живут до срока");
    let fresh = field(&login(&app, "alice", "correct horse").await, "access");
    assert_eq!(authkit::parse(&fresh).unwrap().header.kid, "k2", "новые токены подписывает новый ключ");
    assert!(app.validate_token(&fresh).is_ok(), "и проходят проверку");
}

#[tokio::test]
async fn forged_tokens_are_invalid() {
    let (app, _, _) = app();
    let access = field(&signed_up(&app, "alice").await, "access");
    assert_eq!(app.validate_token("a.b.c"), Err(ValidateError::Invalid), "мусор");
    let mut broken = access.clone();
    broken.push('x');
    assert_eq!(app.validate_token(&broken), Err(ValidateError::Invalid), "испорченная подпись");
    let mut parts: Vec<&str> = access.split('.').collect();
    parts[1] = "eyJzdWIiOiJ1LTEifQ";
    assert_eq!(app.validate_token(&parts.join(".")), Err(ValidateError::Invalid), "подменённые данные");
}

#[tokio::test]
async fn unknown_routes_are_404() {
    let (app, _, _) = app();
    let reply = app.handle(post("/nope", "{}", None)).await;
    assert_eq!(reply.status, 404, "нет такого маршрута");
    let mut get = post("/login", "{}", None);
    get.method = "GET".to_string();
    assert_eq!(app.handle(get).await.status, 404, "вход только по POST");
}
