use std::{
    collections::{HashMap, VecDeque},
    io,
    sync::{Arc, Mutex},
    time::Duration,
};

use hookd::{
    ApiError, BoxFuture, Config, ConfigError, DeadLetter, Hookd, Outgoing, ShutdownReport,
    SubscribeRequest, Transport, TransportError,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::time::{Instant, sleep, timeout};

#[derive(Clone, Copy, Debug)]
enum Reply {
    Status(u16),
    Fail,
    Hang,
}

#[derive(Default)]
struct Fake {
    scripts: Mutex<HashMap<String, VecDeque<Reply>>>,
    sent: Mutex<Vec<(Instant, Outgoing)>>,
}

impl Fake {
    fn script(&self, url: &str, replies: &[Reply]) {
        self.scripts
            .lock()
            .unwrap()
            .insert(url.to_owned(), replies.iter().copied().collect());
    }

    fn sent(&self) -> Vec<(Instant, Outgoing)> {
        self.sent.lock().unwrap().clone()
    }

    fn sent_to(&self, url: &str) -> Vec<(Instant, Outgoing)> {
        self.sent().into_iter().filter(|(_, o)| o.url == url).collect()
    }
}

impl Transport for Fake {
    fn send(&self, request: Outgoing) -> BoxFuture<Result<u16, TransportError>> {
        let reply = self
            .scripts
            .lock()
            .unwrap()
            .get_mut(&request.url)
            .and_then(|q| q.pop_front())
            .unwrap_or(Reply::Status(200));
        self.sent.lock().unwrap().push((Instant::now(), request));
        Box::pin(async move {
            match reply {
                Reply::Status(status) => Ok(status),
                Reply::Fail => Err(TransportError("connection refused".to_owned())),
                Reply::Hang => std::future::pending().await,
            }
        })
    }
}

fn vars(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

fn no_files(_: &str) -> io::Result<String> {
    Err(io::Error::new(io::ErrorKind::NotFound, "нет файла"))
}

fn config(extra: &[(&str, &str)]) -> Config {
    let mut v = vars(&[("HOOKD_SIGNING_KEY", "s3cret")]);
    v.extend(vars(extra));
    Config::from_env(&v, no_files).expect("корректная конфигурация")
}

fn service(extra: &[(&str, &str)]) -> (Hookd, Arc<Fake>) {
    let fake = Arc::new(Fake::default());
    let transport: Arc<dyn Transport> = fake.clone();
    (Hookd::new(config(extra), transport), fake)
}

fn request(url: &str, events: &[&str]) -> SubscribeRequest {
    SubscribeRequest {
        url: url.to_owned(),
        events: events.iter().map(|e| (*e).to_owned()).collect(),
    }
}

async fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    let waited = timeout(Duration::from_secs(120), async {
        while !done() {
            sleep(Duration::from_millis(5)).await;
        }
    })
    .await;
    assert!(waited.is_ok(), "не дождались: {what}");
}

fn hmac(key: &[u8], message: &[u8]) -> String {
    let mut block = [0u8; 64];
    block[..key.len()].copy_from_slice(key);
    let mut inner = Sha256::new();
    inner.update(block.map(|b| b ^ 0x36));
    inner.update(message);
    let inner = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(block.map(|b| b ^ 0x5c));
    outer.update(inner);
    outer
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn gaps(sent: &[(Instant, Outgoing)]) -> Vec<u128> {
    sent.windows(2)
        .map(|w| (w[1].0 - w[0].0).as_millis())
        .collect()
}

#[test]
fn hmac_of_the_test_matches_rfc_4231() {
    assert_eq!(
        hmac(&[0x0b; 20], b"Hi There"),
        "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7",
        "проверочная HMAC в тесте сама верна"
    );
}

#[test]
fn config_reads_the_secret_file_and_defaults() {
    let config = Config::from_env(
        &vars(&[("HOOKD_SIGNING_KEY_FILE", "/run/secrets/hookd")]),
        |path| {
            assert_eq!(path, "/run/secrets/hookd", "путь к секрету берётся из _FILE");
            Ok("k3y\n".to_owned())
        },
    )
    .expect("секрет из файла");
    assert_eq!(config.signing_key, "k3y", "перевод строки в конце файла снимается");
    assert_eq!(config.max_attempts, 5, "по умолчанию 5 попыток");
    assert_eq!(config.backoff, Duration::from_millis(200), "по умолчанию пауза 200 мс");
    assert_eq!(config.max_backoff, Duration::from_secs(10), "по умолчанию потолок паузы 10 с");
    assert_eq!(config.attempt_timeout, Duration::from_secs(2), "по умолчанию срок попытки 2 с");

    let custom = config_with(&[
        ("HOOKD_MAX_ATTEMPTS", "7"),
        ("HOOKD_BACKOFF_MS", "50"),
        ("HOOKD_MAX_BACKOFF_MS", "400"),
        ("HOOKD_ATTEMPT_TIMEOUT_MS", "900"),
    ])
    .expect("свои значения");
    assert_eq!(custom.max_attempts, 7);
    assert_eq!(custom.backoff, Duration::from_millis(50));
    assert_eq!(custom.max_backoff, Duration::from_millis(400));
    assert_eq!(custom.attempt_timeout, Duration::from_millis(900));
}

fn config_with(extra: &[(&str, &str)]) -> Result<Config, ConfigError> {
    let mut v = vars(&[("HOOKD_SIGNING_KEY", "s3cret")]);
    v.extend(vars(extra));
    Config::from_env(&v, no_files)
}

#[test]
fn config_rejects_bad_setups() {
    let key = "HOOKD_SIGNING_KEY".to_owned();
    let both = vars(&[("HOOKD_SIGNING_KEY", "a"), ("HOOKD_SIGNING_KEY_FILE", "/f")]);
    assert_eq!(
        Config::from_env(&both, |_| Ok("b".to_owned())),
        Err(ConfigError::Ambiguous(key.clone())),
        "два источника секрета сразу это ошибка"
    );
    assert_eq!(
        Config::from_env(&HashMap::new(), no_files),
        Err(ConfigError::Missing(key.clone())),
        "без секрета не стартуем"
    );
    assert_eq!(
        Config::from_env(&vars(&[("HOOKD_SIGNING_KEY_FILE", "/nope")]), no_files),
        Err(ConfigError::Unreadable(key.clone())),
        "нечитаемый файл секрета"
    );
    assert_eq!(
        Config::from_env(&vars(&[("HOOKD_SIGNING_KEY_FILE", "/f")]), |_| Ok("\n".to_owned())),
        Err(ConfigError::Empty(key.clone())),
        "пустой секрет это ошибка"
    );
    for (name, value) in [
        ("HOOKD_MAX_ATTEMPTS", "0"),
        ("HOOKD_MAX_ATTEMPTS", "21"),
        ("HOOKD_MAX_ATTEMPTS", "пять"),
        ("HOOKD_BACKOFF_MS", "0"),
        ("HOOKD_ATTEMPT_TIMEOUT_MS", "-1"),
    ] {
        assert_eq!(
            config_with(&[(name, value)]),
            Err(ConfigError::Invalid {
                name: name.to_owned(),
                value: value.to_owned()
            }),
            "{name}={value} недопустимо"
        );
    }
    assert!(
        matches!(
            config_with(&[("HOOKD_BACKOFF_MS", "500"), ("HOOKD_MAX_BACKOFF_MS", "100")]),
            Err(ConfigError::Invalid { ref name, .. }) if name == "HOOKD_MAX_BACKOFF_MS"
        ),
        "потолок паузы меньше самой паузы это ошибка потолка"
    );
}

#[test]
fn config_debug_hides_the_key() {
    let text = format!("{:?}", config(&[]));
    assert!(!text.contains("s3cret"), "ключ подписи не печатается в Debug: {text}");
    assert!(text.contains('5'), "остальные поля в Debug видны: {text}");
}

#[tokio::test(start_paused = true)]
async fn unknown_keys_are_rejected_and_tenants_are_isolated() {
    let (hookd, fake) = service(&[("HOOKD_MAX_ATTEMPTS", "1")]);
    let err = hookd
        .subscribe("hk_wrong", request("https://a.example/hook", &["post.created"]))
        .expect_err("чужой ключ");
    assert_eq!(err, ApiError::Unauthorized);
    assert_eq!(err.status(), 401);
    assert!(hookd.publish("hk_wrong", "k", "post.created", json!({})).is_err(), "публикация с чужим ключом");
    assert_eq!(hookd.dead_letters("hk_wrong"), Err(ApiError::Unauthorized));

    let alice = hookd.create_tenant("alice");
    let bob = hookd.create_tenant("bob");
    assert_ne!(alice, bob, "у арендаторов разные ключи");
    hookd
        .subscribe(&alice, request("https://alice.example/hook", &["post.created"]))
        .expect("подписка alice");
    hookd
        .subscribe(&bob, request("https://bob.example/hook", &["post.created"]))
        .expect("подписка bob");
    fake.script("https://bob.example/hook", &[Reply::Status(500)]);
    hookd.publish(&bob, "k1", "post.created", json!({ "n": 1 })).expect("публикация bob");
    wait_until("событие bob ушло в dead letter", || hookd.stats().dead == 1).await;
    assert!(fake.sent_to("https://alice.example/hook").is_empty(), "событие bob не уходит подпискам alice");
    assert_eq!(fake.sent_to("https://bob.example/hook").len(), 1);
    assert!(hookd.dead_letters(&alice).expect("alice").is_empty(), "alice не видит dead letter bob");
    assert_eq!(hookd.dead_letters(&bob).expect("bob").len(), 1);
}

#[tokio::test(start_paused = true)]
async fn subscription_validation_reports_every_field() {
    let (hookd, _) = service(&[]);
    let key = hookd.create_tenant("t");
    let parsed: SubscribeRequest = serde_json::from_str(
        r#"{"url": "http://x.example/hook", "events": ["post.created", "Bad", "post.created", "*"]}"#,
    )
    .expect("DTO разбирается из JSON");
    let err = hookd.subscribe(&key, parsed).expect_err("три ошибки");
    assert_eq!(err.status(), 400);
    assert_eq!(err.code(), "validation");
    assert_eq!(
        err.body(),
        json!({ "error": {
            "code": "validation",
            "message": err.to_string(),
            "fields": [
                { "path": "url", "code": "scheme" },
                { "path": "events[1]", "code": "format" },
                { "path": "events[2]", "code": "duplicate" },
            ],
        }}),
        "все ошибки сразу, с путями полей, в порядке полей"
    );
    let codes = |r: SubscribeRequest| match hookd.subscribe(&key, r) {
        Err(ApiError::Validation(fields)) => fields
            .into_iter()
            .map(|f| (f.path, f.code))
            .collect::<Vec<_>>(),
        other => panic!("ожидалась ошибка проверки, пришло {other:?}"),
    };
    assert_eq!(codes(request("https://", &[])), [("url".to_owned(), "host"), ("events".to_owned(), "empty")]);
    let many: Vec<String> = (0..21).map(|i| format!("post.e{}", "x".repeat(i + 1))).collect();
    let many: Vec<&str> = many.iter().map(String::as_str).collect();
    assert_eq!(codes(request("https://ok.example", &many)), [("events".to_owned(), "too_many")]);
    let long = format!("https://ok.example/{}", "a".repeat(2048));
    assert_eq!(codes(request(&long, &["*"])), [("url".to_owned(), "too_long")]);
    assert!(hookd.subscribe(&key, request("https://ok.example/h", &["post.created", "*"])).is_ok());
}

#[tokio::test(start_paused = true)]
async fn publish_validates_type_and_key() {
    let (hookd, _) = service(&[]);
    let key = hookd.create_tenant("t");
    match hookd.publish(&key, "", "PostCreated", json!({})) {
        Err(ApiError::Validation(fields)) => {
            let fields: Vec<_> = fields.into_iter().map(|f| (f.path, f.code)).collect();
            assert_eq!(
                fields,
                [("idempotency_key".to_owned(), "format"), ("type".to_owned(), "format")],
                "пустой ключ идемпотентности и тип не вида group.action"
            );
        }
        other => panic!("ожидалась ошибка проверки, пришло {other:?}"),
    }
    let long = "k".repeat(129);
    assert!(hookd.publish(&key, &long, "post.created", json!({})).is_err(), "ключ длиннее 128");
    assert!(hookd.publish(&key, "has space", "post.created", json!({})).is_err(), "ключ с пробелом");
    let ok = hookd.publish(&key, "k1", "post.created", json!({})).expect("событие без подписчиков принимается");
    assert!(!ok.duplicate);
}

#[tokio::test(start_paused = true)]
async fn delivery_is_signed_and_described() {
    let (hookd, fake) = service(&[]);
    let key = hookd.create_tenant("t");
    hookd.subscribe(&key, request("https://a.example/hook", &["post.liked"])).expect("подписка");
    let published = hookd
        .publish(&key, "like-42", "post.liked", json!({ "post": 42, "by": "bob" }))
        .expect("публикация");
    wait_until("доставка", || hookd.stats().delivered == 1).await;
    let sent = fake.sent();
    assert_eq!(sent.len(), 1, "одна доставка");
    let out = &sent[0].1;
    assert_eq!(out.url, "https://a.example/hook");
    let body: Value = serde_json::from_slice(&out.body).expect("тело это JSON");
    assert_eq!(
        body,
        json!({ "id": published.id, "type": "post.liked", "data": { "post": 42, "by": "bob" } }),
        "тело: id, type и data"
    );
    assert_eq!(out.header("content-type"), Some("application/json"));
    assert_eq!(out.header("x-hookd-id"), Some(published.id.as_str()));
    assert_eq!(out.header("x-hookd-event"), Some("post.liked"));
    assert_eq!(out.header("x-hookd-attempt"), Some("1"));
    assert_eq!(
        out.header("x-hookd-signature").map(str::to_owned),
        Some(format!("sha256={}", hmac(b"s3cret", &out.body))),
        "подпись это HMAC-SHA256 ключом подписи от точных байт тела"
    );
}

#[tokio::test(start_paused = true)]
async fn retries_back_off_exponentially_with_a_cap() {
    let (hookd, fake) = service(&[
        ("HOOKD_BACKOFF_MS", "100"),
        ("HOOKD_MAX_BACKOFF_MS", "300"),
        ("HOOKD_MAX_ATTEMPTS", "6"),
    ]);
    let key = hookd.create_tenant("t");
    hookd.subscribe(&key, request("https://a.example/hook", &["*"])).expect("подписка");
    fake.script(
        "https://a.example/hook",
        &[Reply::Status(503), Reply::Status(503), Reply::Status(503), Reply::Status(503), Reply::Status(200)],
    );
    hookd.publish(&key, "k", "user.followed", json!(1)).expect("публикация");
    wait_until("доставка с пятой попытки", || hookd.stats().delivered == 1).await;
    let sent = fake.sent();
    assert_eq!(sent.len(), 5, "пять попыток");
    assert_eq!(gaps(&sent), [100, 200, 300, 300], "паузы 100, 200, потом потолок 300");
    let attempts: Vec<_> = sent.iter().map(|(_, o)| o.header("x-hookd-attempt").unwrap_or("").to_owned()).collect();
    assert_eq!(attempts, ["1", "2", "3", "4", "5"], "номер попытки в заголовке");
    let stats = hookd.stats();
    assert_eq!((stats.published, stats.delivered, stats.failed_attempts, stats.dead), (1, 1, 4, 0));
}

#[tokio::test(start_paused = true)]
async fn client_errors_are_final_but_429_5xx_and_network_errors_are_retried() {
    let (hookd, fake) = service(&[("HOOKD_BACKOFF_MS", "10")]);
    let key = hookd.create_tenant("t");
    for (url, replies) in [
        ("https://bad.example", vec![Reply::Status(400), Reply::Status(200)]),
        ("https://moved.example", vec![Reply::Status(301), Reply::Status(200)]),
        ("https://busy.example", vec![Reply::Status(429), Reply::Status(200)]),
        ("https://down.example", vec![Reply::Fail, Reply::Status(502), Reply::Status(204)]),
    ] {
        hookd.subscribe(&key, request(url, &["*"])).expect("подписка");
        fake.script(url, &replies);
    }
    hookd.publish(&key, "k", "post.created", json!(null)).expect("публикация");
    wait_until("все четыре доставки решены", || {
        let s = hookd.stats();
        s.delivered + s.dead == 4
    })
    .await;
    sleep(Duration::from_secs(5)).await;
    assert_eq!(fake.sent_to("https://bad.example").len(), 1, "400 не повторяется");
    assert_eq!(fake.sent_to("https://moved.example").len(), 1, "3xx не повторяется");
    assert_eq!(fake.sent_to("https://busy.example").len(), 2, "429 повторяется");
    assert_eq!(fake.sent_to("https://down.example").len(), 3, "обрыв и 5xx повторяются");
    let mut dead = hookd.dead_letters(&key).expect("dead letters");
    dead.sort_by_key(|d| d.last_status);
    assert_eq!(
        dead.iter().map(|d| (d.attempts, d.last_status)).collect::<Vec<_>>(),
        [(1, Some(301)), (1, Some(400))],
        "отказы клиента сразу в dead letter с одной попыткой"
    );
}

#[tokio::test(start_paused = true)]
async fn gives_up_after_max_attempts() {
    let (hookd, fake) = service(&[("HOOKD_MAX_ATTEMPTS", "3"), ("HOOKD_BACKOFF_MS", "10")]);
    let key = hookd.create_tenant("t");
    let sub = hookd.subscribe(&key, request("https://a.example", &["post.created"])).expect("подписка");
    fake.script("https://a.example", &[Reply::Status(500), Reply::Fail, Reply::Status(500), Reply::Status(200)]);
    let published = hookd.publish(&key, "k", "post.created", json!({})).expect("публикация");
    wait_until("dead letter", || hookd.stats().dead == 1).await;
    sleep(Duration::from_secs(60)).await;
    assert_eq!(fake.sent().len(), 3, "после трёх попыток больше не пытаемся");
    assert_eq!(
        hookd.dead_letters(&key).expect("dead letters"),
        [DeadLetter { event_id: published.id, subscription: sub, attempts: 3, last_status: Some(500) }]
    );
}

#[tokio::test(start_paused = true)]
async fn hanging_endpoint_hits_the_attempt_timeout() {
    let (hookd, fake) = service(&[("HOOKD_ATTEMPT_TIMEOUT_MS", "1000"), ("HOOKD_BACKOFF_MS", "100")]);
    let key = hookd.create_tenant("t");
    hookd.subscribe(&key, request("https://slow.example", &["*"])).expect("подписка");
    fake.script("https://slow.example", &[Reply::Hang, Reply::Status(200)]);
    hookd.publish(&key, "k", "post.created", json!({})).expect("публикация");
    wait_until("доставка после зависшей попытки", || hookd.stats().delivered == 1).await;
    assert_eq!(gaps(&fake.sent()), [1100], "вторая попытка через срок попытки и паузу");
}

#[tokio::test(start_paused = true)]
async fn events_keep_their_order_per_subscription() {
    let (hookd, fake) = service(&[("HOOKD_BACKOFF_MS", "50")]);
    let key = hookd.create_tenant("t");
    hookd.subscribe(&key, request("https://a.example", &["*"])).expect("подписка");
    fake.script("https://a.example", &[Reply::Status(503), Reply::Status(503), Reply::Status(200)]);
    let first = hookd.publish(&key, "k1", "post.created", json!(1)).expect("первое");
    let second = hookd.publish(&key, "k2", "post.deleted", json!(2)).expect("второе");
    let third = hookd.publish(&key, "k3", "post.created", json!(3)).expect("третье");
    wait_until("три доставки", || hookd.stats().delivered == 3).await;
    let ids: Vec<_> = fake.sent().iter().map(|(_, o)| o.header("x-hookd-id").unwrap_or("").to_owned()).collect();
    assert_eq!(
        ids,
        [first.id.as_str(), first.id.as_str(), first.id.as_str(), second.id.as_str(), third.id.as_str()],
        "следующее событие подписки уходит только после того, как решилась судьба предыдущего"
    );
}

#[tokio::test(start_paused = true)]
async fn a_stuck_subscription_does_not_block_the_others() {
    let (hookd, fake) = service(&[("HOOKD_ATTEMPT_TIMEOUT_MS", "30000")]);
    let key = hookd.create_tenant("t");
    hookd.subscribe(&key, request("https://stuck.example", &["*"])).expect("подписка");
    hookd.subscribe(&key, request("https://fine.example", &["*"])).expect("подписка");
    fake.script("https://stuck.example", &[Reply::Hang; 5]);
    let start = Instant::now();
    for i in 0..3 {
        hookd.publish(&key, &format!("k{i}"), "post.created", json!(i)).expect("публикация");
    }
    wait_until("здоровая подписка получила всё", || fake.sent_to("https://fine.example").len() == 3).await;
    assert!(start.elapsed() < Duration::from_secs(1), "здоровая подписка не ждёт зависшую");
    assert_eq!(fake.sent_to("https://stuck.example").len(), 1, "зависшая всё ещё на первой попытке");
}

#[tokio::test(start_paused = true)]
async fn idempotency_key_dedupes_and_detects_conflicts() {
    let (hookd, fake) = service(&[]);
    let alice = hookd.create_tenant("alice");
    let bob = hookd.create_tenant("bob");
    hookd.subscribe(&alice, request("https://a.example", &["*"])).expect("подписка");
    let first = hookd.publish(&alice, "order-7", "post.created", json!({ "a": 1, "b": 2 })).expect("первая");
    let again = hookd.publish(&alice, "order-7", "post.created", json!({ "b": 2, "a": 1 })).expect("повтор");
    assert_eq!(again.id, first.id, "повтор с тем же ключом возвращает то же событие");
    assert!(!first.duplicate && again.duplicate, "повтор помечен как дубль");
    let conflict = hookd
        .publish(&alice, "order-7", "post.created", json!({ "a": 2 }))
        .expect_err("тот же ключ, другие данные");
    assert_eq!(conflict, ApiError::IdempotencyConflict);
    assert_eq!(conflict.status(), 409);
    assert_eq!(conflict.body()["error"]["code"], "idempotency_conflict");
    assert!(conflict.body()["error"].get("fields").is_none(), "fields только у ошибок проверки");
    assert!(
        hookd.publish(&alice, "order-7", "post.deleted", json!({ "a": 1, "b": 2 })).is_err(),
        "тот же ключ, другой тип"
    );
    let other = hookd.publish(&bob, "order-7", "post.created", json!({ "a": 1, "b": 2 })).expect("bob");
    assert!(!other.duplicate && other.id != first.id, "ключи идемпотентности у арендаторов свои");
    wait_until("доставка", || hookd.stats().delivered == 1).await;
    sleep(Duration::from_secs(5)).await;
    assert_eq!(fake.sent().len(), 1, "дубль не доставляется второй раз");
    assert_eq!(hookd.stats().published, 2, "дубли не считаются публикациями");
}

#[tokio::test(start_paused = true)]
async fn shutdown_drains_then_refuses() {
    let (hookd, fake) = service(&[("HOOKD_BACKOFF_MS", "100")]);
    let key = hookd.create_tenant("t");
    hookd.subscribe(&key, request("https://a.example", &["*"])).expect("подписка");
    fake.script("https://a.example", &[Reply::Status(503), Reply::Status(200)]);
    hookd.publish(&key, "k1", "post.created", json!({})).expect("публикация");
    hookd.publish(&key, "k2", "post.created", json!({})).expect("публикация");
    let report = hookd.shutdown(Duration::from_secs(5)).await;
    assert_eq!(report, ShutdownReport { drained: true, undelivered: 0 }, "начатая работа доделана");
    assert_eq!(hookd.stats().delivered, 2);
    let refused = hookd.publish(&key, "k3", "post.created", json!({})).expect_err("после остановки");
    assert_eq!(refused, ApiError::ShuttingDown);
    assert_eq!(refused.status(), 503);
    assert_eq!(
        hookd.subscribe(&key, request("https://b.example", &["*"])),
        Err(ApiError::ShuttingDown),
        "новые подписки после остановки тоже не принимаются"
    );
}

#[tokio::test(start_paused = true)]
async fn shutdown_gives_up_after_grace() {
    let (hookd, fake) = service(&[("HOOKD_ATTEMPT_TIMEOUT_MS", "60000")]);
    let key = hookd.create_tenant("t");
    hookd.subscribe(&key, request("https://stuck.example", &["*"])).expect("подписка");
    hookd.subscribe(&key, request("https://fine.example", &["*"])).expect("подписка");
    fake.script("https://stuck.example", &[Reply::Hang]);
    for i in 0..2 {
        hookd.publish(&key, &format!("k{i}"), "post.created", json!(i)).expect("публикация");
    }
    let start = Instant::now();
    let report = hookd.shutdown(Duration::from_secs(1)).await;
    let took = start.elapsed();
    assert!(
        took >= Duration::from_secs(1) && took < Duration::from_millis(1500),
        "остановка ждёт ровно grace, а не зависшую попытку: {took:?}"
    );
    assert_eq!(report, ShutdownReport { drained: false, undelivered: 2 }, "у зависшей подписки осталось два события");
    assert_eq!(fake.sent_to("https://fine.example").len(), 2, "здоровая подписка успела всё");
    sleep(Duration::from_secs(120)).await;
    assert_eq!(fake.sent_to("https://stuck.example").len(), 1, "после остановки попыток больше нет");
}
