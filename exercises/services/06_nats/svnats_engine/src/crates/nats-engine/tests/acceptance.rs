use async_nats::{Client, HeaderMap, Subscriber};
use futures::StreamExt;
use nats_engine::{Bus, BusConfig, BusError, Meta, Pattern, Seg, Subject, SubjectError};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio::time::timeout;

const DEFAULT_URL: &str = "nats://127.0.0.1:54222";

fn url() -> String {
    std::env::var("RUSTLINGS_NATS_URL").unwrap_or_else(|_| DEFAULT_URL.to_string())
}

fn unique() -> String {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
    format!("t_{}_{nanos:x}_{}", std::process::id(), COUNTER.fetch_add(1, Ordering::Relaxed))
}

async fn bus() -> Bus {
    Bus::connect(BusConfig::new(url())).await.expect("шина подключилась")
}

async fn raw() -> Client {
    async_nats::connect(url()).await.expect("сырое подключение")
}

async fn raw_sub(client: &Client, subject: impl async_nats::subject::ToSubject) -> Subscriber {
    let sub = client.subscribe(subject).await.unwrap();
    let inbox = client.new_inbox();
    let mut probe = client.subscribe(inbox.clone()).await.unwrap();
    client.publish(inbox, "".into()).await.unwrap();
    probe.next().await;
    sub
}

fn subject(root: &str, tail: &[&str]) -> Subject {
    let mut tokens = vec![root];
    tokens.extend_from_slice(tail);
    Subject::new(&tokens).expect("тема")
}

fn pattern(root: &str, tail: &[Seg<'_>]) -> Pattern {
    let mut segments = vec![Seg::Word(root)];
    segments.extend_from_slice(tail);
    Pattern::build(&segments).expect("шаблон")
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Liked {
    post_id: u64,
    by: String,
}

fn liked(post_id: u64) -> Liked {
    Liked { post_id, by: "bob".to_string() }
}

const WAIT: Duration = Duration::from_secs(2);

// ---------- темы ----------

#[test]
fn subjects_are_built_from_checked_words() {
    let s = Subject::new(&["social", "post", "liked", "alice"]).unwrap();
    assert_eq!(s.as_str(), "social.post.liked.alice", "слова соединяются точками");
    assert_eq!(s.last_token(), "alice", "последнее слово");
    assert_eq!(Subject::parse("a.b.c").unwrap().as_str(), "a.b.c", "parse разбирает готовую тему");
    assert_eq!(Subject::new(&[]), Err(SubjectError::NoTokens), "тема без слов");
    assert_eq!(Subject::new(&["a", ""]), Err(SubjectError::Empty), "пустое слово");
    assert_eq!(Subject::new(&["a", "b.c"]), Err(SubjectError::BadChar('.')), "точка внутри слова");
    assert_eq!(Subject::new(&["a", "*"]), Err(SubjectError::BadChar('*')), "звёздочка в теме для публикации");
    assert_eq!(Subject::new(&["a", ">"]), Err(SubjectError::BadChar('>')), "треугольная скобка");
    assert_eq!(Subject::new(&["a", "x y"]), Err(SubjectError::BadChar(' ')), "пробел");
    assert_eq!(Subject::new(&["a", "x\r\ny"]), Err(SubjectError::BadChar('\r')), "перевод строки");
    assert_eq!(Subject::new(&["a", &"x".repeat(33)]), Err(SubjectError::TooLong), "слишком длинное слово");
    assert!(Subject::new(&["a", &"x".repeat(32)]).is_ok(), "32 символа ещё можно");
    for bad in ["", "a..b", ".a", "a.", "a.*", "a.>"] {
        assert!(Subject::parse(bad).is_err(), "{bad:?} не тема для публикации");
    }
}

#[test]
fn wildcards_come_only_from_explicit_segments() {
    let p = Pattern::build(&[Seg::Word("social"), Seg::Any, Seg::Word("liked"), Seg::Tail]).unwrap();
    assert_eq!(p.as_str(), "social.*.liked.>", "шаблон из явных сегментов");
    assert_eq!(Pattern::build(&[Seg::Word("*")]), Err(SubjectError::BadChar('*')), "звёздочка в слове это не подстановка, а ошибка");
    assert_eq!(Pattern::build(&[Seg::Word("a"), Seg::Word(">")]), Err(SubjectError::BadChar('>')), "то же для `>`");
    assert_eq!(Pattern::build(&[Seg::Tail, Seg::Word("a")]), Err(SubjectError::BadWildcard), "`>` только последним");
    assert_eq!(Pattern::build(&[]), Err(SubjectError::NoTokens), "пустой шаблон");
    assert_eq!(Pattern::parse("social.*.>").unwrap().as_str(), "social.*.>", "parse принимает готовую строку из конфига");
    for bad in ["", "a..b", "a.>.b", "a.b*", "a b", ".a"] {
        assert!(Pattern::parse(bad).is_err(), "{bad:?} не шаблон");
    }
}

#[test]
fn patterns_match_like_the_broker() {
    let s = |raw: &str| Subject::parse(raw).unwrap();
    let any = Pattern::parse("social.*.created").unwrap();
    assert!(any.matches(&s("social.post.created")), "звёздочка берёт ровно одно слово");
    assert!(!any.matches(&s("social.created")), "ноль слов не годится");
    assert!(!any.matches(&s("social.post.x.created")), "два слова не годится");
    let tail = Pattern::parse("social.>").unwrap();
    assert!(tail.matches(&s("social.post.liked.alice")), "хвост берёт всё остальное");
    assert!(!tail.matches(&s("social")), "но хотя бы одно слово");
    assert!(!tail.matches(&s("notify.post")), "начало должно совпасть");
    assert!(Pattern::parse("a.b").unwrap().matches(&s("a.b")) && !Pattern::parse("a.b").unwrap().matches(&s("a.b.c")), "точное совпадение");
}

// ---------- метаданные ----------

#[test]
fn meta_accepts_only_safe_identifiers() {
    assert_eq!(Meta::new("req-1", "trace_2").unwrap(), Meta { request_id: "req-1".to_string(), trace_id: "trace_2".to_string() }, "годные идентификаторы");
    for bad in ["", "a b", "x:y", "a\r\nX-Evil: 1", &"z".repeat(65)] {
        assert!(Meta::new(bad, "t").is_err(), "request_id {bad:?} негоден");
        assert!(Meta::new("r", bad).is_err(), "trace_id {bad:?} негоден");
    }
    let (a, b) = (Meta::fresh(), Meta::fresh());
    assert_ne!(a.request_id, b.request_id, "свежие request_id разные");
    assert!(Meta::new(&a.request_id, &a.trace_id).is_ok(), "свежие идентификаторы сами проходят проверку");
}

// ---------- publish и subscribe ----------

#[tokio::test]
async fn typed_event_travels_with_its_meta() {
    let root = unique();
    let bus = bus().await;
    let mut sub = bus.subscribe::<Liked>(&pattern(&root, &[Seg::Word("post"), Seg::Tail])).await.expect("подписка");
    let meta = Meta::new("req-7", "trace-7").unwrap();
    let topic = subject(&root, &["post", "liked", "alice"]);
    bus.publish(&topic, &liked(5), &meta).await.expect("публикация");
    let got = sub.next_within(WAIT).await.expect("событие пришло").expect("событие разобралось");
    assert_eq!(got.body, liked(5), "тело");
    assert_eq!(got.subject, topic, "тема");
    assert_eq!(got.meta, Some(meta), "идентификаторы доехали заголовками");
}

#[tokio::test]
async fn headers_on_the_wire_are_exactly_the_two_ids() {
    let root = unique();
    let bus = bus().await;
    let watcher = raw().await;
    let mut sub = raw_sub(&watcher, format!("{root}.>")).await;
    bus.publish(&subject(&root, &["x"]), &liked(1), &Meta::new("req-1", "trace-1").unwrap()).await.unwrap();
    let message = timeout(WAIT, sub.next()).await.expect("сообщение").expect("поток");
    let headers = message.headers.expect("заголовки");
    assert_eq!(headers.get("X-Request-Id").map(|v| v.as_str()), Some("req-1"), "X-Request-Id");
    assert_eq!(headers.get("X-Trace-Id").map(|v| v.as_str()), Some("trace-1"), "X-Trace-Id");
    assert_eq!(headers.iter().count(), 2, "никаких других заголовков");
    assert_eq!(message.payload.as_ref(), br#"{"post_id":1,"by":"bob"}"#, "тело это JSON значения");
}

#[tokio::test]
async fn subscription_filters_by_pattern() {
    let root = unique();
    let bus = bus().await;
    let mut sub = bus.subscribe::<Liked>(&pattern(&root, &[Seg::Word("post"), Seg::Any, Seg::Word("alice")])).await.unwrap();
    let meta = Meta::fresh();
    bus.publish(&subject(&root, &["post", "liked", "alice"]), &liked(1), &meta).await.unwrap();
    bus.publish(&subject(&root, &["post", "liked", "bob"]), &liked(2), &meta).await.unwrap();
    bus.publish(&subject(&root, &["user", "followed", "alice"]), &liked(3), &meta).await.unwrap();
    bus.publish(&subject(&root, &["post", "commented", "alice"]), &liked(4), &meta).await.unwrap();
    let first = sub.next_within(WAIT).await.unwrap().unwrap();
    let second = sub.next_within(WAIT).await.unwrap().unwrap();
    assert_eq!((first.body.post_id, second.body.post_id), (1, 4), "приходят только подходящие, по порядку");
    assert!(sub.next_within(Duration::from_millis(250)).await.is_none(), "остальное не доходит");
}

#[tokio::test]
async fn a_broken_message_is_one_error_and_the_stream_goes_on() {
    let root = unique();
    let bus = bus().await;
    let mut sub = bus.subscribe::<Liked>(&pattern(&root, &[Seg::Tail])).await.unwrap();
    let outsider = raw().await;
    outsider.publish(format!("{root}.junk"), "это не JSON".into()).await.unwrap();
    outsider.publish(format!("{root}.junk"), r#"{"post_id":"строка","by":1}"#.into()).await.unwrap();
    let mut headers = HeaderMap::new();
    headers.insert("X-Request-Id", "req-9");
    headers.insert("X-Trace-Id", "trace-9");
    outsider.publish_with_headers(format!("{root}.good"), headers, r#"{"post_id":9,"by":"bob"}"#.into()).await.unwrap();
    for n in 1..=2 {
        let item = sub.next_within(WAIT).await.expect("сообщение пришло");
        assert!(matches!(item, Err(BusError::Decode(_))), "сообщение {n} сломано: ждали Err(Decode), получили {item:?}");
    }
    let good = sub.next_within(WAIT).await.expect("третье сообщение").expect("оно целое");
    assert_eq!(good.body, liked(9), "после мусора поток продолжается");
    assert_eq!(good.meta, Some(Meta::new("req-9", "trace-9").unwrap()), "идентификаторы на месте");
}

#[tokio::test]
async fn messages_without_ids_still_arrive_with_empty_meta() {
    let root = unique();
    let bus = bus().await;
    let mut sub = bus.subscribe::<Liked>(&pattern(&root, &[Seg::Tail])).await.unwrap();
    let outsider = raw().await;
    outsider.publish(format!("{root}.plain"), r#"{"post_id":1,"by":"x"}"#.into()).await.unwrap();
    let mut evil = HeaderMap::new();
    evil.insert("X-Request-Id", "bad id with spaces");
    evil.insert("X-Trace-Id", "t1");
    outsider.publish_with_headers(format!("{root}.evil"), evil, r#"{"post_id":2,"by":"x"}"#.into()).await.unwrap();
    let plain = sub.next_within(WAIT).await.unwrap().unwrap();
    assert_eq!(plain.meta, None, "сообщение без заголовков имеет meta = None");
    let evil = sub.next_within(WAIT).await.unwrap().unwrap();
    assert_eq!(evil.meta, None, "негодные идентификаторы чужого отправителя не принимаются на веру");
}

#[tokio::test]
async fn subscribe_returns_only_when_the_broker_already_knows_the_subscription() {
    let proxy = Proxy::start().await;
    proxy.set_latency(Duration::from_millis(150));
    let root = unique();
    let slow_bus = Bus::connect(BusConfig::new(proxy.url())).await.expect("шина через медленную связь");
    let mut sub = slow_bus.subscribe::<Liked>(&pattern(&root, &[Seg::Tail])).await.unwrap();
    let fast = bus().await;
    fast.publish(&subject(&root, &["post"]), &liked(1), &Meta::fresh()).await.unwrap();
    fast.close().await.unwrap();
    let got = sub.next_within(Duration::from_secs(3)).await;
    assert!(matches!(got, Some(Ok(_))), "событие сразу после subscribe потерялось: дождись подтверждения брокера до возврата");
}

#[tokio::test]
async fn a_stopped_subscription_still_hands_over_what_it_already_has() {
    let root = unique();
    let bus = bus().await;
    let mut sub = bus.subscribe::<Liked>(&pattern(&root, &[Seg::Tail])).await.unwrap();
    let outsider = raw().await;
    for i in 0..5 {
        outsider.publish(format!("{root}.post"), format!(r#"{{"post_id":{i},"by":"x"}}"#).into()).await.unwrap();
    }
    outsider.flush().await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    sub.stop().await;
    outsider.publish(format!("{root}.post"), r#"{"post_id":99,"by":"x"}"#.into()).await.unwrap();
    outsider.flush().await.unwrap();
    let mut ids = Vec::new();
    while let Some(item) = timeout(WAIT, sub.next()).await.expect("поток после stop должен закончиться, а не повиснуть") {
        ids.push(item.unwrap().body.post_id);
    }
    assert_eq!(ids, vec![0, 1, 2, 3, 4], "после stop можно дочитать то, что уже лежало в буфере; события, опубликованные позже, не приходят");
}

// ---------- queue groups ----------

#[tokio::test]
async fn a_group_shares_events_and_other_groups_get_their_own_copy() {
    let root = unique();
    let (members, audit_bus) = (bus().await, bus().await);
    let topic = pattern(&root, &[Seg::Word("post"), Seg::Tail]);
    let mut group = Vec::new();
    for _ in 0..3 {
        group.push(members.subscribe_group::<Liked>(&topic, "notify").await.expect("участник группы"));
    }
    let mut feed = audit_bus.subscribe_group::<Liked>(&topic, "feed").await.expect("другая группа");
    assert!(members.subscribe_group::<Liked>(&topic, "no.dots").await.is_err(), "имя группы тоже проверяется как слово");
    let publisher = bus().await;
    for i in 0..60 {
        publisher.publish(&subject(&root, &["post", "liked"]), &liked(i), &Meta::fresh()).await.unwrap();
    }
    let mut seen = BTreeSet::new();
    let mut per_member = Vec::new();
    for member in &mut group {
        let mut mine = 0;
        while let Some(item) = member.next_within(Duration::from_millis(400)).await {
            assert!(seen.insert(item.unwrap().body.post_id), "событие досталось двум участникам группы сразу");
            mine += 1;
        }
        per_member.push(mine);
    }
    assert_eq!(seen.len(), 60, "группа вместе получила все 60 событий, по разу: {per_member:?}");
    let mut feed_seen = BTreeSet::new();
    while let Some(item) = feed.next_within(Duration::from_millis(400)).await {
        feed_seen.insert(item.unwrap().body.post_id);
    }
    assert_eq!(feed_seen.len(), 60, "группа feed получила собственную полную копию");
}

// ---------- request и serve ----------

#[derive(Debug, Serialize, Deserialize)]
struct Ask {
    user: String,
}

#[tokio::test]
async fn request_reply_roundtrip_with_a_typed_server() {
    let root = unique();
    let service = bus().await;
    let seen_ids = Arc::new(Mutex::new(Vec::new()));
    let log = seen_ids.clone();
    let _server = service
        .serve::<Ask, u32, _>(&pattern(&root, &[Seg::Word("unread"), Seg::Any]), None, move |request| {
            log.lock().unwrap().push(request.meta.map(|m| m.request_id));
            Ok(request.subject.last_token().len() as u32 * 10)
        })
        .await
        .expect("ответчик");
    let gateway = bus().await;
    let meta = Meta::new("req-42", "trace-42").unwrap();
    let answer: u32 = gateway.request(&subject(&root, &["unread", "alice"]), &Ask { user: "alice".to_string() }, &meta, WAIT).await.expect("ответ");
    assert_eq!(answer, 50, "ответ по теме запроса");
    let answer: u32 = gateway.request(&subject(&root, &["unread", "bo"]), &Ask { user: "bo".to_string() }, &meta, WAIT).await.unwrap();
    assert_eq!(answer, 20, "второй запрос");
    assert_eq!(*seen_ids.lock().unwrap(), vec![Some("req-42".to_string()); 2], "обработчик видит request-id запроса");
}

#[tokio::test]
async fn request_failures_are_told_apart() {
    let root = unique();
    let gateway = bus().await;
    let meta = Meta::fresh();
    let started = Instant::now();
    let nobody: Result<u32, _> = gateway.request(&subject(&root, &["unread", "alice"]), &Ask { user: "a".to_string() }, &meta, Duration::from_secs(2)).await;
    assert_eq!(nobody, Err(BusError::NoService), "слушателей нет");
    assert!(started.elapsed() < Duration::from_millis(800), "NoService приходит сразу, а не по истечении срока");
    let silent = raw().await;
    let mut hole = raw_sub(&silent, format!("{root}.silent.>")).await;
    tokio::spawn(async move { while hole.next().await.is_some() {} });
    let started = Instant::now();
    let late: Result<u32, _> = gateway.request(&subject(&root, &["silent", "alice"]), &Ask { user: "a".to_string() }, &meta, Duration::from_millis(300)).await;
    assert_eq!(late, Err(BusError::Timeout), "слушатель молчит");
    let spent = started.elapsed();
    assert!(spent >= Duration::from_millis(280) && spent < Duration::from_millis(1500), "срок соблюдается: {spent:?}");
    let service = bus().await;
    let _server = service
        .serve::<Ask, u32, _>(&pattern(&root, &[Seg::Word("strict"), Seg::Any]), None, |request| {
            if request.body.user == "ghost" { Err("нет такого пользователя".to_string()) } else { Ok(1) }
        })
        .await
        .unwrap();
    let refused: Result<u32, _> = gateway.request(&subject(&root, &["strict", "x"]), &Ask { user: "ghost".to_string() }, &meta, WAIT).await;
    assert_eq!(refused, Err(BusError::Remote("нет такого пользователя".to_string())), "ошибка обработчика доезжает как Remote");
    let fine: Result<u32, _> = gateway.request(&subject(&root, &["strict", "x"]), &Ask { user: "alice".to_string() }, &meta, WAIT).await;
    assert_eq!(fine, Ok(1), "после ошибки сервер продолжает отвечать");
}

#[tokio::test]
async fn a_server_survives_garbage_requests() {
    let root = unique();
    let service = bus().await;
    let _server = service
        .serve::<Ask, u32, _>(&pattern(&root, &[Seg::Word("unread"), Seg::Any]), None, |_| Ok(7))
        .await
        .unwrap();
    let outsider = raw().await;
    outsider.publish(format!("{root}.unread.x"), "нет адреса ответа".into()).await.unwrap();
    let reply = timeout(WAIT, outsider.request(format!("{root}.unread.x"), "не JSON".into())).await.expect("ответ").expect("запрос");
    assert!(String::from_utf8_lossy(&reply.payload).contains("Err"), "мусор получает ответ-ошибку, а не тишину: {:?}", reply.payload);
    let gateway = bus().await;
    let answer: u32 = gateway.request(&subject(&root, &["unread", "x"]), &Ask { user: "a".to_string() }, &Meta::fresh(), WAIT).await.unwrap();
    assert_eq!(answer, 7, "и после мусора обычные запросы работают");
}

#[tokio::test]
async fn two_server_instances_in_a_group_answer_each_request_once() {
    let root = unique();
    let counter = Arc::new(AtomicU64::new(0));
    let (first, second) = (bus().await, bus().await);
    let topic = pattern(&root, &[Seg::Word("count")]);
    let mut servers = Vec::new();
    for bus in [&first, &second] {
        let counter = counter.clone();
        servers.push(
            bus.serve::<Ask, u32, _>(&topic, Some("counter"), move |_| {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(1)
            })
            .await
            .unwrap(),
        );
    }
    let gateway = bus().await;
    for _ in 0..20 {
        let _: u32 = gateway.request(&subject(&root, &["count"]), &Ask { user: "a".to_string() }, &Meta::fresh(), WAIT).await.unwrap();
    }
    assert_eq!(counter.load(Ordering::SeqCst), 20, "20 запросов обработаны ровно 20 раз на два экземпляра");
    for server in servers {
        server.stop().await;
    }
    let after: Result<u32, _> = gateway.request(&subject(&root, &["count"]), &Ask { user: "a".to_string() }, &Meta::fresh(), WAIT).await;
    assert_eq!(after, Err(BusError::NoService), "остановленные серверы больше не слушают");
}

// ---------- связь, перегрузка, выход ----------

#[tokio::test]
async fn readiness_and_publishing_follow_the_connection() {
    let proxy = Proxy::start().await;
    let root = unique();
    let watcher = raw().await;
    let mut sub = raw_sub(&watcher, format!("{root}.>")).await;
    let bus = Bus::connect(BusConfig::new(proxy.url())).await.expect("шина");
    assert!(bus.ready(), "после подключения готова");
    proxy.cut();
    let deadline = Instant::now() + Duration::from_secs(2);
    while bus.ready() {
        assert!(Instant::now() < deadline, "обрыв не замечен за 2 секунды: ready() всё ещё true");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let topic = subject(&root, &["post"]);
    let flood = async {
        for i in 0..3000 {
            let result = bus.publish(&topic, &liked(i), &Meta::fresh()).await;
            assert_eq!(result, Err(BusError::Unavailable), "публикация {i} без связи должна сразу отказать");
        }
        let asked: Result<u32, _> = bus.request(&topic, &liked(0), &Meta::fresh(), Duration::from_secs(5)).await;
        assert_eq!(asked, Err(BusError::Unavailable), "запрос без связи тоже отказывает сразу");
    };
    timeout(Duration::from_secs(2), flood).await.expect("без связи публикация или запрос зависли");
    proxy.restore();
    let deadline = Instant::now() + Duration::from_secs(6);
    while !bus.ready() {
        assert!(Instant::now() < deadline, "связь не вернулась за 6 секунд");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    bus.publish(&topic, &liked(1), &Meta::fresh()).await.expect("после возврата связи");
    let first = timeout(WAIT, sub.next()).await.expect("событие").expect("поток");
    assert_eq!(first.payload.as_ref(), br#"{"post_id":1,"by":"bob"}"#, "первым пришло событие, опубликованное при связи, а не накопленное в скрытом буфере");
    assert!(timeout(Duration::from_millis(400), sub.next()).await.is_err(), "ничего накопленного за время обрыва нет");
}

#[tokio::test]
async fn slow_consumers_are_counted() {
    let root = unique();
    let config = BusConfig { subscription_capacity: 16, ..BusConfig::new(url()) };
    let bus = Bus::connect(config).await.unwrap();
    let mut sub = bus.subscribe::<Liked>(&pattern(&root, &[Seg::Tail])).await.unwrap();
    assert_eq!(bus.slow_consumer_events(), 0, "до перегрузки нуль");
    let publisher = raw().await;
    for i in 0..300 {
        publisher.publish(format!("{root}.post"), format!(r#"{{"post_id":{i},"by":"x"}}"#).into()).await.unwrap();
    }
    publisher.flush().await.unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(bus.slow_consumer_events() > 0, "буфер на 16 переполнился, а шина этого не заметила");
    let mut got = 0;
    while sub.next_within(Duration::from_millis(200)).await.is_some() {
        got += 1;
    }
    assert!(got < 300, "часть событий должна была потеряться, получено {got}");
}

#[tokio::test]
async fn close_delivers_what_was_published_just_before() {
    let root = unique();
    let watcher = bus().await;
    let mut sub = watcher.subscribe::<Liked>(&pattern(&root, &[Seg::Tail])).await.unwrap();
    for i in 0..10 {
        let short_lived = bus().await;
        short_lived.publish(&subject(&root, &["post"]), &liked(i), &Meta::fresh()).await.unwrap();
        short_lived.close().await.expect("закрытие");
        let got = sub.next_within(WAIT).await.expect("событие пришло").expect("разобралось");
        assert_eq!(got.body.post_id, i, "событие {i} из короткоживущего процесса не должно теряться");
    }
}

// ---------- прокси для тестов связи ----------

struct Proxy {
    addr: SocketAddr,
    cut: Arc<AtomicBool>,
    kill: broadcast::Sender<()>,
    latency_ms: Arc<AtomicU64>,
    task: tokio::task::JoinHandle<()>,
}

impl Proxy {
    async fn start() -> Proxy {
        let upstream = url();
        let upstream = upstream.strip_prefix("nats://").unwrap_or(&upstream).to_string();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let cut = Arc::new(AtomicBool::new(false));
        let latency_ms = Arc::new(AtomicU64::new(0));
        let (kill, _) = broadcast::channel(1);
        let task = tokio::spawn({
            let (cut, kill, latency_ms) = (cut.clone(), kill.clone(), latency_ms.clone());
            async move {
                while let Ok((client, _)) = listener.accept().await {
                    if cut.load(Ordering::SeqCst) {
                        continue;
                    }
                    let (mut stop, upstream, latency_ms) = (kill.subscribe(), upstream.clone(), latency_ms.clone());
                    tokio::spawn(async move {
                        let Ok(server) = TcpStream::connect(&upstream).await else { return };
                        let (mut from_client, mut to_client) = client.into_split();
                        let (mut from_server, mut to_server) = server.into_split();
                        let up = async move {
                            let mut chunk = [0u8; 16 * 1024];
                            loop {
                                let n = match from_client.read(&mut chunk).await {
                                    Ok(0) | Err(_) => break,
                                    Ok(n) => n,
                                };
                                let delay = latency_ms.load(Ordering::SeqCst);
                                if delay > 0 {
                                    tokio::time::sleep(Duration::from_millis(delay)).await;
                                }
                                if to_server.write_all(&chunk[..n]).await.is_err() {
                                    break;
                                }
                            }
                        };
                        let down = async move {
                            tokio::io::copy(&mut from_server, &mut to_client).await.ok();
                        };
                        tokio::select! { _ = up => {} _ = down => {} _ = stop.recv() => {} }
                    });
                }
            }
        });
        Proxy { addr, cut, kill, latency_ms, task }
    }

    fn url(&self) -> String {
        format!("nats://{}", self.addr)
    }

    fn cut(&self) {
        self.cut.store(true, Ordering::SeqCst);
        self.kill.send(()).ok();
    }

    fn restore(&self) {
        self.cut.store(false, Ordering::SeqCst);
    }

    fn set_latency(&self, delay: Duration) {
        self.latency_ms.store(delay.as_millis() as u64, Ordering::SeqCst);
    }
}

impl Drop for Proxy {
    fn drop(&mut self) {
        self.task.abort();
        self.kill.send(()).ok();
    }
}
