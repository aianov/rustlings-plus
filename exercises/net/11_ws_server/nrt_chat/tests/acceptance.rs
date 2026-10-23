use std::future::Future;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use chat_gateway::{Config, Stats, Tokens, run};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio::time::{sleep, timeout};
use websocket_engine::{MaskGen, Message, Role, WsConfig, WsStream, client_request, connect, new_key};

async fn within<T>(what: &str, fut: impl Future<Output = T>) -> T {
    match timeout(Duration::from_secs(8), fut).await {
        Ok(v) => v,
        Err(_) => panic!("{what}: за 8 секунд ничего не произошло"),
    }
}

async fn eventually(what: &str, mut check: impl FnMut() -> bool) {
    let began = Instant::now();
    while !check() {
        if began.elapsed() > Duration::from_secs(6) {
            panic!("{what}: за 6 секунд условие не выполнилось");
        }
        sleep(Duration::from_millis(20)).await;
    }
}

struct Fixed;

impl Tokens for Fixed {
    fn check(&self, token: &str) -> Option<String> {
        match token {
            "alice-token" => Some("alice".to_string()),
            "bob-token" => Some("bob".to_string()),
            "carol-token" => Some("carol".to_string()),
            _ => None,
        }
    }
}

struct Server {
    addr: SocketAddr,
    stats: Arc<Stats>,
    stop: Option<oneshot::Sender<()>>,
    task: JoinHandle<std::io::Result<()>>,
}

async fn start(config: Config) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let stats = Arc::new(Stats::default());
    let (stop, rx) = oneshot::channel::<()>();
    let task = tokio::spawn(run(listener, Arc::new(Fixed), config, stats.clone(), async move {
        let _ = rx.await;
    }));
    Server { addr, stats, stop: Some(stop), task }
}

fn quick() -> Config {
    Config { ping_every: Duration::from_secs(60), idle_limit: Duration::from_secs(600), grace: Duration::from_secs(2), ..Config::default() }
}

struct Client {
    ws: WsStream<TcpStream>,
    next_id: u64,
    events: Vec<Value>,
}

async fn login(server: &Server, who: &str) -> Client {
    let path = format!("/ws?token={who}-token");
    let ws = within("подключение", connect(server.addr, "chat.test", &path, None, &WsConfig::default())).await.unwrap_or_else(|e| panic!("{who} не смог подключиться: {e}"));
    Client { ws, next_id: 0, events: Vec::new() }
}

impl Client {
    async fn recv_json(&mut self) -> Value {
        match within("ожидание сообщения от сервера", self.ws.recv()).await {
            Ok(Some(Message::Text(text))) => serde_json::from_str(&text).unwrap_or_else(|e| panic!("сервер прислал не JSON ({e}): {text}")),
            other => panic!("ждали текстовое сообщение, получили {other:?}"),
        }
    }

    async fn send_call(&mut self, method: &str, data: Value) -> u64 {
        self.next_id += 1;
        let (service, name) = method.split_once('.').unwrap();
        let text = json!({ "id": self.next_id, "service": service, "method": name, "data": data }).to_string();
        within("отправка запроса", self.ws.send(Message::Text(text))).await.unwrap();
        self.next_id
    }

    async fn reply_for(&mut self, id: u64) -> Value {
        loop {
            let message = self.recv_json().await;
            if message.get("id") == Some(&json!(id)) {
                return message;
            }
            assert!(message.get("event").is_some(), "пришло сообщение, которое не ответ на {id} и не событие: {message}");
            self.events.push(message);
        }
    }

    async fn call(&mut self, method: &str, data: Value) -> Value {
        let id = self.send_call(method, data).await;
        self.reply_for(id).await
    }

    async fn ok(&mut self, method: &str, data: Value) -> Value {
        let reply = self.call(method, data).await;
        assert_eq!(reply["ok"], json!(true), "{method} вернул ошибку: {reply}");
        reply["data"].clone()
    }

    async fn next_event(&mut self) -> Value {
        if !self.events.is_empty() {
            return self.events.remove(0);
        }
        let message = self.recv_json().await;
        assert!(message.get("event").is_some(), "ждали событие, пришло {message}");
        message
    }

    async fn expect_silence(&mut self) {
        if let Ok(message) = timeout(Duration::from_millis(250), self.ws.recv()).await {
            panic!("сервер не должен был ничего присылать, а прислал {message:?}");
        }
    }
}

async fn raw_handshake(addr: SocketAddr, path: &str, extra: &str) -> (String, TcpStream) {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = format!("GET {path} HTTP/1.1\r\nHost: chat.test\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n{extra}\r\n");
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut head = Vec::new();
    let mut chunk = [0u8; 1024];
    while !head.windows(4).any(|w| w == b"\r\n\r\n") {
        let n = within("ответ на рукопожатие", stream.read(&mut chunk)).await.unwrap();
        assert!(n > 0, "сервер закрыл соединение, не ответив");
        head.extend_from_slice(&chunk[..n]);
    }
    (String::from_utf8_lossy(&head).into_owned(), stream)
}

#[tokio::test]
async fn bad_tokens_are_refused_with_401_before_the_upgrade() {
    let server = start(quick()).await;
    for (path, extra) in [("/ws", ""), ("/ws?token=", ""), ("/ws?token=mallory-token", ""), ("/ws", "Authorization: Bearer mallory-token\r\n"), ("/ws", "Authorization: Basic YWxpY2U6cHc=\r\n")] {
        let (head, _stream) = raw_handshake(server.addr, path, extra).await;
        assert!(head.starts_with("HTTP/1.1 401"), "для {path:?} {extra:?} ждали 401 до апгрейда, получили:\n{head}");
        assert!(!head.to_ascii_lowercase().contains("sec-websocket-accept"), "плохой токен не должен получить рукопожатие");
    }
    assert_eq!(server.stats.connections(), 0, "отказанные клиенты не должны попадать в реестр");
    let (head, _stream) = raw_handshake(server.addr, "/nope?token=alice-token", "").await;
    assert!(head.starts_with("HTTP/1.1 404"), "чужой путь это 404:\n{head}");
}

#[tokio::test]
async fn token_from_query_or_from_bearer_header_opens_the_socket() {
    let server = start(quick()).await;
    let (head, _s1) = raw_handshake(server.addr, "/ws?room=1&token=alice-token", "").await;
    assert!(head.starts_with("HTTP/1.1 101"), "токен в адресе, ждали 101:\n{head}");
    let (head, _s2) = raw_handshake(server.addr, "/ws", "Authorization: Bearer bob-token\r\n").await;
    assert!(head.starts_with("HTTP/1.1 101"), "токен в заголовке Authorization, ждали 101:\n{head}");
    let (head, _s3) = raw_handshake(server.addr, "/ws?token=mallory-token", "Authorization: bearer carol-token\r\n").await;
    assert!(head.starts_with("HTTP/1.1 101"), "схема Bearer не зависит от регистра и главнее адреса:\n{head}");
    eventually("пользователи в реестре", || server.stats.connections() == 3).await;
}

#[tokio::test]
async fn whoami_unknown_methods_and_garbage() {
    let server = start(quick()).await;
    let mut alice = login(&server, "alice").await;
    assert_eq!(alice.ok("sys.whoami", Value::Null).await, json!({ "user": "alice" }));

    let reply = alice.call("nope.method", Value::Null).await;
    assert_eq!((reply["ok"].clone(), reply["error"]["code"].clone()), (json!(false), json!("not_found")), "{reply}");

    within("мусор", alice.ws.send(Message::Text("это не json".to_string()))).await.unwrap();
    let reply = alice.recv_json().await;
    assert_eq!((reply["id"].clone(), reply["ok"].clone(), reply["error"]["code"].clone()), (Value::Null, json!(false), json!("bad_json")), "{reply}");

    within("запрос без метода", alice.ws.send(Message::Text(r#"{"id":77,"service":"chat"}"#.to_string()))).await.unwrap();
    let reply = alice.recv_json().await;
    assert_eq!((reply["id"].clone(), reply["error"]["code"].clone()), (json!(77), json!("bad_request")), "ошибка в запросе с известным id должна нести этот id: {reply}");

    within("двоичное сообщение", alice.ws.send(Message::Binary(vec![1, 2, 3]))).await.unwrap();
    let reply = alice.recv_json().await;
    assert_eq!(reply["ok"], json!(false), "двоичные сообщения не поддерживаются, но соединение остаётся: {reply}");

    assert_eq!(alice.ok("sys.whoami", Value::Null).await, json!({ "user": "alice" }), "после мусора соединение должно жить");
}

#[tokio::test]
async fn rooms_chat_push_and_sequence_numbers() {
    let server = start(quick()).await;
    let mut alice = login(&server, "alice").await;
    let mut bob = login(&server, "bob").await;
    let mut carol = login(&server, "carol").await;

    let joined = alice.ok("chat.join", json!({ "room": "kitchen" })).await;
    assert_eq!(joined, json!({ "room": "kitchen", "members": ["alice"] }));
    let joined = bob.ok("chat.join", json!({ "room": "kitchen" })).await;
    assert_eq!(joined["members"], json!(["alice", "bob"]), "участники отсортированы по имени");

    for n in 1..=3 {
        let said = alice.ok("chat.say", json!({ "room": "kitchen", "text": format!("чай номер {n}") })).await;
        assert_eq!(said, json!({ "delivered": 1 }), "доставлено одному устройству Боба");
    }
    for n in 1..=3 {
        let event = bob.next_event().await;
        assert_eq!(event["event"], json!("chat.message"));
        assert_eq!(event["data"], json!({ "room": "kitchen", "from": "alice", "text": format!("чай номер {n}") }));
        assert_eq!(event["seq"], json!(n), "события нумеруются с единицы подряд внутри соединения");
        assert!(event.get("id").is_none(), "у события нет id");
    }
    alice.expect_silence().await;
    carol.expect_silence().await;

    let who = carol.ok("chat.who", json!({ "room": "kitchen" })).await;
    assert_eq!(who["members"], json!(["alice", "bob"]), "смотреть состав комнаты может любой");
    let who = carol.ok("chat.who", json!({ "room": "пустая" })).await;
    assert_eq!(who["members"], json!([]));

    let reply = carol.call("chat.say", json!({ "room": "kitchen", "text": "я не в комнате" })).await;
    assert_eq!(reply["error"]["code"], json!("not_member"), "{reply}");
    bob.expect_silence().await;

    assert_eq!(bob.ok("chat.leave", json!({ "room": "kitchen" })).await, json!({ "left": true }));
    assert_eq!(bob.ok("chat.leave", json!({ "room": "kitchen" })).await, json!({ "left": false }));
    let said = alice.ok("chat.say", json!({ "room": "kitchen", "text": "ушёл" })).await;
    assert_eq!(said, json!({ "delivered": 0 }));
    bob.expect_silence().await;

    let reply = alice.call("chat.say", json!({ "room": "kitchen", "text": "" })).await;
    assert_eq!(reply["error"]["code"], json!("too_long"), "пустой текст не принимается: {reply}");
    let reply = alice.call("chat.say", json!({ "room": "kitchen", "text": "я".repeat(262_145) })).await;
    assert_eq!(reply["error"]["code"], json!("too_long"), "{}", reply["error"]);
    let reply = alice.call("chat.join", json!({ "room": "" })).await;
    assert_eq!(reply["error"]["code"], json!("bad_request"));
}

#[tokio::test]
async fn every_device_of_every_member_gets_the_message_except_the_sending_one() {
    let server = start(quick()).await;
    let mut a_phone = login(&server, "alice").await;
    let mut a_laptop = login(&server, "alice").await;
    let mut b_phone = login(&server, "bob").await;
    let mut b_laptop = login(&server, "bob").await;
    a_phone.ok("chat.join", json!({ "room": "r" })).await;
    b_laptop.ok("chat.join", json!({ "room": "r" })).await;

    let said = a_phone.ok("chat.say", json!({ "room": "r", "text": "всем" })).await;
    assert_eq!(said, json!({ "delivered": 3 }), "Алиса с телефона: ноутбук Алисы и оба устройства Боба (отправляющее соединение пропускается)");
    for device in [&mut a_laptop, &mut b_phone] {
        let event = device.next_event().await;
        assert_eq!(event["data"]["text"], json!("всем"));
        assert_eq!(event["seq"], json!(1));
    }
    let event = b_laptop.next_event().await;
    assert_eq!(event["data"]["from"], json!("alice"));
    a_phone.expect_silence().await;

    let said = a_laptop.ok("chat.say", json!({ "room": "r", "text": "с ноутбука" })).await;
    assert_eq!(said, json!({ "delivered": 3 }));
    let event = a_phone.next_event().await;
    assert_eq!((event["data"]["text"].clone(), event["seq"].clone()), (json!("с ноутбука"), json!(1)), "у телефона свой счётчик событий");
    let event = b_phone.next_event().await;
    assert_eq!(event["seq"], json!(2));
}

#[tokio::test]
async fn disconnects_clean_up_connections_and_membership() {
    let server = start(quick()).await;
    let mut alice = login(&server, "alice").await;
    let mut bob1 = login(&server, "bob").await;
    let bob2 = login(&server, "bob").await;
    alice.ok("chat.join", json!({ "room": "r" })).await;
    bob1.ok("chat.join", json!({ "room": "r" })).await;
    assert_eq!(server.stats.connections(), 3);

    drop(bob1);
    eventually("первое устройство Боба должно исчезнуть", || server.stats.connections() == 2).await;
    let who = alice.ok("chat.who", json!({ "room": "r" })).await;
    assert_eq!(who["members"], json!(["alice", "bob"]), "второе устройство Боба ещё на связи, значит Боб остаётся в комнате");

    drop(bob2);
    eventually("последнее устройство Боба должно исчезнуть", || server.stats.connections() == 1).await;
    let who = alice.ok("chat.who", json!({ "room": "r" })).await;
    assert_eq!(who["members"], json!(["alice"]), "Боба больше нет ни на одном устройстве: он вышел из комнат");

    drop(alice);
    eventually("все соединения закрыты", || server.stats.connections() == 0).await;
    let mut carol = login(&server, "carol").await;
    let who = carol.ok("chat.who", json!({ "room": "r" })).await;
    assert_eq!(who["members"], json!([]), "пустая комната должна исчезнуть");
}

#[tokio::test]
async fn requests_of_one_connection_run_in_parallel_and_replies_come_when_ready() {
    let server = start(quick()).await;
    let mut alice = login(&server, "alice").await;
    let began = Instant::now();
    let slow = alice.send_call("sys.sleep", json!({ "ms": 600 })).await;
    let fast = alice.send_call("sys.sleep", json!({ "ms": 100 })).await;
    let middle = alice.send_call("sys.sleep", json!({ "ms": 300 })).await;
    let mut order = Vec::new();
    for _ in 0..3 {
        let reply = alice.recv_json().await;
        assert_eq!(reply["ok"], json!(true), "{reply}");
        order.push(reply["id"].as_u64().unwrap());
    }
    assert_eq!(order, vec![fast, middle, slow], "ответы должны идти по готовности, каждый со своим id");
    assert!(began.elapsed() < Duration::from_millis(950), "три запроса заняли {:?}: по очереди выходит 1 с, параллельно около 0,6 с", began.elapsed());
    let reply = alice.call("sys.sleep", json!({ "ms": 99999 })).await;
    assert_eq!(reply["error"]["code"], json!("bad_request"));
}

#[tokio::test]
async fn in_flight_limit_applies_per_connection() {
    let server = start(Config { max_in_flight: 2, ..quick() }).await;
    let mut alice = login(&server, "alice").await;
    let began = Instant::now();
    for _ in 0..6 {
        alice.send_call("sys.sleep", json!({ "ms": 200 })).await;
    }
    for _ in 0..6 {
        let reply = alice.recv_json().await;
        assert_eq!(reply["ok"], json!(true), "{reply}");
    }
    let took = began.elapsed();
    assert!(took >= Duration::from_millis(560), "шесть запросов по 200 мс при лимите 2 это три волны, не меньше 0,6 с, а заняло {took:?}: лимит не соблюдается");
    assert!(took < Duration::from_millis(1150), "лимит 2 должен давать параллельность 2 (около 0,6 с), а заняло {took:?}");
    assert_eq!(alice.ok("sys.whoami", Value::Null).await, json!({ "user": "alice" }));
}

#[tokio::test]
async fn slow_reader_is_kicked_and_nobody_else_notices() {
    let server = start(Config { outbox: 8, ..quick() }).await;
    let mut alice = login(&server, "alice").await;
    let mut bob = login(&server, "bob").await;
    let mut carol = login(&server, "carol").await;
    for client in [&mut alice, &mut bob, &mut carol] {
        client.ok("chat.join", json!({ "room": "big" })).await;
    }
    let received = Arc::new(AtomicUsize::new(0));
    let order = Arc::new(Mutex::new(Vec::new()));
    let (counter, seen) = (received.clone(), order.clone());
    let reader = tokio::spawn(async move {
        while let Ok(Some(Message::Text(text))) = alice.ws.recv().await {
            let event: Value = serde_json::from_str(&text).unwrap();
            let head: String = event["data"]["text"].as_str().unwrap().chars().take(4).collect();
            seen.lock().unwrap().push(head.parse::<usize>().unwrap_or(usize::MAX));
            counter.fetch_add(1, Ordering::SeqCst);
        }
    });
    let filler = "x".repeat(250_000);
    let mut sent = 0;
    while server.stats.kicked() == 0 {
        assert!(sent < 300, "за 300 сообщений по 250 КБ медленный клиент не отключён: очередь не ограничена или отключение не срабатывает");
        let text = format!("{sent:04}{filler}");
        bob.ok("chat.say", json!({ "room": "big", "text": text })).await;
        sent += 1;
    }
    assert_eq!(server.stats.kicked(), 1, "отключён должен быть ровно один клиент");
    eventually("быстрый клиент получил всё", || received.load(Ordering::SeqCst) == sent).await;
    eventually("медленный клиент убран из реестра", || server.stats.connections() == 2).await;
    let said = bob.ok("chat.say", json!({ "room": "big", "text": "после" })).await;
    assert_eq!(said, json!({ "delivered": 1 }), "Кэрол отключена, осталась только Алиса");
    eventually("Алиса получила и последнее", || received.load(Ordering::SeqCst) == sent + 1).await;
    let order = order.lock().unwrap().clone();
    let expected: Vec<usize> = (0..sent).chain([usize::MAX]).collect();
    assert_eq!(order, expected, "быстрый клиент должен получить всё по порядку");
    reader.abort();
    drop(carol);
}

#[tokio::test]
async fn silent_clients_are_dropped_and_clients_answering_pings_stay() {
    let config = Config { ping_every: Duration::from_millis(100), idle_limit: Duration::from_millis(500), ..quick() };
    let server = start(config).await;
    let alive = login(&server, "alice").await;
    let silent = login(&server, "bob").await;
    let mut alive = alive;
    let keeper = tokio::spawn(async move { while let Ok(Some(_)) = alive.ws.recv().await {} });
    eventually("молчун должен быть отключён", || server.stats.timed_out() == 1).await;
    eventually("молчун должен исчезнуть из реестра", || server.stats.connections() == 1).await;
    sleep(Duration::from_millis(1200)).await;
    assert_eq!(server.stats.connections(), 1, "клиент, который отвечает на ping, должен жить сколько угодно");
    assert_eq!(server.stats.timed_out(), 1);
    assert!(!keeper.is_finished(), "соединение живого клиента закрыто");
    drop(silent);
}

#[tokio::test]
async fn shutdown_says_1001_to_everyone_and_finishes_in_time() {
    let mut server = start(quick()).await;
    let mut alice = login(&server, "alice").await;
    let mut bob = login(&server, "bob").await;
    assert_eq!(alice.ok("sys.whoami", Value::Null).await, json!({ "user": "alice" }));
    server.stop.take().unwrap().send(()).unwrap();
    for client in [&mut alice, &mut bob] {
        let end = within("клиент должен получить Close", client.ws.recv()).await;
        assert!(matches!(end, Ok(None)), "ждали аккуратное закрытие, получили {end:?}");
        assert_eq!(client.ws.close_code(), Some(1001), "остановка сервера объявляется кодом 1001");
    }
    let finished = timeout(Duration::from_secs(5), &mut server.task).await.expect("run не завершился после остановки");
    finished.unwrap().unwrap();
    assert_eq!(server.stats.connections(), 0);
}

#[tokio::test]
async fn bearer_client_built_by_hand_works_like_the_library_one() {
    let server = start(quick()).await;
    let mut stream = TcpStream::connect(server.addr).await.unwrap();
    let key = new_key(&mut MaskGen::new());
    let request = client_request("chat.test", "/ws", &key, None).replace("\r\n\r\n", "\r\nAuthorization: Bearer alice-token\r\n\r\n");
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
        let n = within("ответ", stream.read(&mut chunk)).await.unwrap();
        assert!(n > 0);
        buf.extend_from_slice(&chunk[..n]);
    }
    let end = buf.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
    let rest = buf.split_off(end);
    assert!(String::from_utf8_lossy(&buf).starts_with("HTTP/1.1 101"));
    let mut client = Client { ws: WsStream::new(stream, rest, Role::Client, &WsConfig::default()), next_id: 0, events: Vec::new() };
    assert_eq!(client.ok("sys.whoami", Value::Null).await, json!({ "user": "alice" }));
}
