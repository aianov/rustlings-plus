use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use nats_engine::js::{StreamSpec, ensure_stream, history, live, open_bucket, publish_event};
use nats_engine::{Bus, BusConfig, DEFAULT_NATS_URL, Meta, Subject};
use serde::{Deserialize, Serialize};

const WAIT: Duration = Duration::from_secs(5);

fn url() -> String {
    std::env::var("RUSTLINGS_NATS_URL")
        .ok()
        .filter(|url| !url.is_empty())
        .unwrap_or_else(|| DEFAULT_NATS_URL.to_string())
}

fn root() -> String {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.subsec_nanos())
        .unwrap_or(0);
    format!(
        "g{}_{nanos:x}_{}",
        std::process::id() % 100_000,
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

async fn bus() -> Bus {
    Bus::connect(BusConfig::new(url()))
        .await
        .expect("NATS курса с JetStream запущен?")
}

async fn js() -> async_nats::jetstream::Context {
    async_nats::jetstream::new(async_nats::connect(url()).await.expect("сырое подключение"))
}

async fn readers(name: &str) -> usize {
    let mut stream = js().await.get_stream(name).await.expect("stream есть");
    stream
        .info()
        .await
        .expect("сведения о stream")
        .state
        .consumer_count
}

async fn drop_stream(name: &str) {
    let _ = js().await.delete_stream(name).await;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Letter {
    text: String,
}

fn letter(text: &str) -> Letter {
    Letter {
        text: text.to_string(),
    }
}

fn subject(root: &str, tail: &[&str]) -> Subject {
    let mut words = vec![root];
    words.extend_from_slice(tail);
    Subject::new(&words).expect("тема")
}

fn chat(root: &str) -> StreamSpec {
    StreamSpec::new(
        &format!("{}_CHAT", root.to_ascii_uppercase()),
        vec![format!("{root}.chat.>")],
    )
    .republish(&format!("{root}.chat.>"), &format!("{root}.live.>"))
}

#[tokio::test]
async fn a_stored_message_is_echoed_live_with_its_sequence_once() {
    let bus = bus().await;
    let root = root();
    let spec = chat(&root);
    ensure_stream(&bus, &spec).await.expect("stream");
    ensure_stream(&bus, &spec)
        .await
        .expect("повторный ensure_stream ничего не ломает");
    let mut alice = live::<Letter>(&bus, &subject(&root, &["live", "u_1"]))
        .await
        .expect("живая подписка");
    let to_alice = subject(&root, &["chat", "u_1"]);
    let meta = Meta::fresh();
    let first = publish_event(&bus, &to_alice, "m-1", &letter("раз"), &meta)
        .await
        .expect("запись");
    let again = publish_event(&bus, &to_alice, "m-1", &letter("раз"), &meta)
        .await
        .expect("повтор");
    assert!(again.duplicate && again.sequence == first.sequence);
    let second = publish_event(
        &bus,
        &subject(&root, &["chat", "u_1"]),
        "m-2",
        &letter("два"),
        &meta,
    )
    .await
    .expect("вторая запись");
    publish_event(
        &bus,
        &subject(&root, &["chat", "u_2"]),
        "m-3",
        &letter("чужое"),
        &meta,
    )
    .await
    .expect("чужая запись");
    let mut seen = Vec::new();
    for _ in 0..2 {
        let stored = tokio::time::timeout(WAIT, alice.next())
            .await
            .expect("живое сообщение пришло")
            .expect("подписка жива");
        let body = stored.item.expect("сообщение разобрано").body;
        seen.push((stored.sequence, body.text));
    }
    assert_eq!(
        seen,
        [
            (first.sequence, String::from("раз")),
            (second.sequence, String::from("два"))
        ],
        "брокер повторяет в живую тему только записанное и с его номером: дубль по Nats-Msg-Id не записан и не повторён"
    );
    let extra = tokio::time::timeout(Duration::from_millis(300), alice.next()).await;
    assert!(extra.is_err(), "ни дубля, ни чужого: {:?}", extra.ok());
    alice.stop().await;
    drop_stream(&spec.name).await;
}

#[tokio::test]
async fn history_reads_after_a_sequence_and_leaves_no_reader() {
    let bus = bus().await;
    let root = root();
    let spec = chat(&root);
    ensure_stream(&bus, &spec).await.expect("stream");
    let meta = Meta::fresh();
    let mine = subject(&root, &["chat", "u_1"]);
    let mut sequences = Vec::new();
    for n in 1..=5 {
        let text = format!("m{n}");
        let published = publish_event(&bus, &mine, &format!("id-{n}"), &letter(&text), &meta)
            .await
            .expect("запись");
        sequences.push(published.sequence);
        publish_event(
            &bus,
            &subject(&root, &["chat", "u_2"]),
            &format!("x-{n}"),
            &letter("чужое"),
            &meta,
        )
        .await
        .expect("чужая запись");
    }
    let after = sequences[1];
    let page = history::<Letter>(&bus, &spec.name, &mine, after, 2)
        .await
        .expect("история");
    let texts: Vec<(u64, String)> = page
        .into_iter()
        .map(|stored| (stored.sequence, stored.item.expect("разобрано").body.text))
        .collect();
    assert_eq!(
        texts,
        [
            (sequences[2], String::from("m3")),
            (sequences[3], String::from("m4"))
        ],
        "только свои сообщения, строго после номера, по порядку, не больше limit; номера это номера stream"
    );
    let rest = history::<Letter>(&bus, &spec.name, &mine, sequences[3], 10)
        .await
        .expect("продолжение");
    assert_eq!(rest.len(), 1, "страницу продолжают с последнего номера");
    let none = history::<Letter>(&bus, &spec.name, &mine, sequences[4], 10)
        .await
        .expect("пусто");
    assert!(
        none.is_empty(),
        "после последнего ничего нет, и запрос не ждёт таймаута"
    );
    assert_eq!(
        readers(&spec.name).await,
        0,
        "временный читатель удаляется после запроса"
    );
    drop_stream(&spec.name).await;
}

#[tokio::test]
async fn a_bucket_key_lives_while_touched_and_expires_without_beats() {
    let bus = bus().await;
    let name = format!("{}_PRESENCE", root().to_ascii_uppercase());
    let ttl = Duration::from_millis(1000);
    let bucket = open_bucket(&bus, &name, ttl).await.expect("бакет");
    let again = open_bucket(&bus, &name, ttl).await;
    assert!(again.is_ok(), "повторное открытие берёт тот же бакет");
    for key in ["u_1.gw-a-1", "u_1.gw-b-1", "u_2.gw-a-2"] {
        bucket.touch(key).await.expect("сигнал");
    }
    assert_eq!(
        bucket.alive("u_1").await,
        Ok(2),
        "у u_1 два живых устройства"
    );
    assert_eq!(
        bucket.alive("u_2").await,
        Ok(1),
        "u_2 только что прислал сигнал"
    );
    bucket.forget("u_1.gw-b-1").await.expect("ушёл");
    assert_eq!(
        bucket.alive("u_1").await,
        Ok(1),
        "закрытое устройство ушло, другое живо"
    );
    for _ in 0..8 {
        tokio::time::sleep(Duration::from_millis(300)).await;
        bucket.touch("u_1.gw-a-1").await.expect("сигнал");
    }
    assert_eq!(
        bucket.alive("u_1").await,
        Ok(1),
        "дольше срока жизни, но с сигналами каждые 300 мс: устройство живо"
    );
    let began = std::time::Instant::now();
    while bucket.alive("u_2").await != Ok(0) {
        assert!(
            began.elapsed() < WAIT,
            "запись без сигналов не истекла: упавший шлюз оставил бы пользователя в сети навсегда"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(
        readers(&format!("KV_{name}")).await,
        0,
        "подсчёт не оставляет читателей"
    );
    drop_stream(&format!("KV_{name}")).await;
}
