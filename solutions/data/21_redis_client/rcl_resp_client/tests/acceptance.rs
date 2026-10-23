use std::io::{Read, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use resp_client::{Client, Error, Options, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{sleep, timeout};

fn redis_url() -> String {
    std::env::var("RUSTLINGS_REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:56379".to_string())
}

fn redis_addr() -> String {
    redis_url().trim_start_matches("redis://").trim_end_matches('/').to_string()
}

static COUNTER: AtomicUsize = AtomicUsize::new(0);

struct Scope {
    prefix: String,
}

impl Scope {
    fn new() -> Scope {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        Scope { prefix: format!("rc_{}_{}_{}", std::process::id(), n, std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0)) }
    }

    fn key(&self, name: &str) -> String {
        format!("{}:{name}", self.prefix)
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        let script = "local c='0' repeat local r=redis.call('SCAN',c,'MATCH',ARGV[1],'COUNT',1000) c=r[1] for _,k in ipairs(r[2]) do redis.call('UNLINK',k) end until c=='0' return 1";
        let pattern = format!("{}:*", self.prefix);
        if let Ok(mut stream) = std::net::TcpStream::connect(redis_addr()) {
            let mut wire = Vec::new();
            let args = ["EVAL", script, "0", pattern.as_str()];
            wire.extend_from_slice(format!("*{}\r\n", args.len()).as_bytes());
            for a in args {
                wire.extend_from_slice(format!("${}\r\n{a}\r\n", a.len()).as_bytes());
            }
            let _ = stream.write_all(&wire);
            let mut reply = [0u8; 16];
            let _ = stream.read(&mut reply);
        }
    }
}

enum Step {
    Write(Vec<u8>),
    Sleep(Duration),
    Close,
}

struct Fake {
    addr: String,
    connections: Arc<AtomicUsize>,
    open: Arc<AtomicUsize>,
    log: Arc<Mutex<Vec<Vec<String>>>>,
    task: tokio::task::JoinHandle<()>,
}

impl Fake {
    async fn start<F>(handler: F) -> Fake
    where
        F: Fn(usize, Vec<String>) -> Vec<Step> + Send + Sync + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("порт");
        let addr = listener.local_addr().expect("адрес").to_string();
        let connections = Arc::new(AtomicUsize::new(0));
        let open = Arc::new(AtomicUsize::new(0));
        let log = Arc::new(Mutex::new(Vec::new()));
        let handler = Arc::new(handler);
        let (c, o, l) = (connections.clone(), open.clone(), log.clone());
        let task = tokio::spawn(async move {
            let mut set = tokio::task::JoinSet::new();
            while let Ok((stream, _)) = listener.accept().await {
                let index = c.fetch_add(1, Ordering::SeqCst);
                o.fetch_add(1, Ordering::SeqCst);
                let (handler, o, l) = (handler.clone(), o.clone(), l.clone());
                set.spawn(async move {
                    serve(stream, index, handler, l).await;
                    o.fetch_sub(1, Ordering::SeqCst);
                });
            }
        });
        Fake { addr, connections, open, log, task }
    }

    fn url(&self) -> String {
        format!("redis://{}", self.addr)
    }

    fn connections(&self) -> usize {
        self.connections.load(Ordering::SeqCst)
    }

    fn open(&self) -> usize {
        self.open.load(Ordering::SeqCst)
    }

    fn commands(&self) -> Vec<Vec<String>> {
        self.log.lock().expect("журнал").clone()
    }
}

impl Drop for Fake {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn read_command(buf: &[u8]) -> Option<(Vec<String>, usize)> {
    fn line(buf: &[u8], from: usize) -> Option<(&str, usize)> {
        let rel = buf.get(from..)?.windows(2).position(|w| w == b"\r\n")?;
        Some((std::str::from_utf8(&buf[from..from + rel]).ok()?, from + rel + 2))
    }
    let (head, mut pos) = line(buf, 0)?;
    let count: usize = head.strip_prefix('*')?.parse().ok()?;
    let mut args = Vec::new();
    for _ in 0..count {
        let (len_line, next) = line(buf, pos)?;
        let len: usize = len_line.strip_prefix('$')?.parse().ok()?;
        let body = buf.get(next..next + len + 2)?;
        args.push(String::from_utf8_lossy(&body[..len]).into_owned());
        pos = next + len + 2;
    }
    Some((args, pos))
}

async fn serve<F>(mut stream: TcpStream, index: usize, handler: Arc<F>, log: Arc<Mutex<Vec<Vec<String>>>>)
where
    F: Fn(usize, Vec<String>) -> Vec<Step> + Send + Sync + 'static,
{
    let mut buf: Vec<u8> = Vec::new();
    loop {
        while let Some((args, used)) = read_command(&buf) {
            buf.drain(..used);
            log.lock().expect("журнал").push(args.clone());
            for step in handler(index, args) {
                match step {
                    Step::Write(bytes) => {
                        if stream.write_all(&bytes).await.is_err() {
                            return;
                        }
                    }
                    Step::Sleep(d) => sleep(d).await,
                    Step::Close => return,
                }
            }
        }
        let mut chunk = [0u8; 4096];
        match stream.read(&mut chunk).await {
            Ok(0) | Err(_) => return,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
    }
}

fn say(text: &str) -> Vec<Step> {
    vec![Step::Write(text.as_bytes().to_vec())]
}

fn quick() -> Options {
    Options { connect_timeout: Duration::from_millis(500), command_timeout: Duration::from_millis(150), reconnect_pause: Duration::from_millis(50), ..Options::default() }
}

fn cmd(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| s.to_string()).collect()
}

#[tokio::test]
async fn url_selects_the_database() {
    let client = Client::connect(&format!("{}/9", redis_url().trim_end_matches('/'))).await.expect("подключение к базе 9");
    match client.call(&["CLIENT", "INFO"]).await.expect("CLIENT INFO") {
        Value::Bulk(info) => assert!(String::from_utf8_lossy(&info).contains(" db=9 "), "соединение должно работать в базе 9"),
        other => panic!("CLIENT INFO вернул {other:?}"),
    }
}

#[tokio::test]
async fn auth_and_select_go_first_on_every_connection() {
    let server = Fake::start(|_, _| say("+OK\r\n")).await;
    let url = format!("redis://:p%40ss@{}/3", server.addr);
    let _client = Client::connect(&url).await.expect("подключение");
    assert_eq!(server.commands(), vec![cmd(&["AUTH", "p@ss"]), cmd(&["SELECT", "3"]), cmd(&["PING"])], "при подключении: AUTH с раскодированным паролем, SELECT, PING");
}

#[tokio::test]
async fn bad_url_is_refused_before_connecting() {
    for bad in ["http://localhost", "redis://", "redis://host:port", "redis://host/x"] {
        match Client::connect(bad).await {
            Err(Error::InvalidUrl(_)) => {}
            Err(other) => panic!("для адреса {bad:?} ожидалась InvalidUrl, а получено {other}"),
            Ok(_) => panic!("адрес {bad:?} неправильный, а подключение удалось"),
        }
    }
}

#[tokio::test]
async fn silent_server_times_out_the_connect() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("порт");
    let addr = listener.local_addr().expect("адрес").to_string();
    let holder = tokio::spawn(async move {
        let mut keep = Vec::new();
        while let Ok((stream, _)) = listener.accept().await {
            keep.push(stream);
        }
    });
    let res = timeout(Duration::from_secs(3), Client::connect_with(&format!("redis://{addr}"), quick())).await.expect("подключение не уложилось в собственный срок");
    assert!(matches!(res, Err(Error::Timeout)), "сервер принял TCP и молчит: ожидалась Error::Timeout");
    holder.abort();
}

#[tokio::test]
async fn typed_commands() {
    let scope = Scope::new();
    let client = Client::connect(&redis_url()).await.expect("подключение");
    let (name, bin, count, tmp) = (scope.key("name"), scope.key("bin"), scope.key("count"), scope.key("tmp"));
    assert_eq!(client.get(&name).await.expect("GET"), None, "ключа нет");
    client.set(&name, "Мария".as_bytes()).await.expect("SET");
    assert_eq!(client.get(&name).await.expect("GET"), Some("Мария".as_bytes().to_vec()), "значение вернулось");
    client.set(&name, b"").await.expect("SET пустого");
    assert_eq!(client.get(&name).await.expect("GET"), Some(Vec::new()), "пустое значение это Some, а не None");
    let raw = vec![0u8, 255, b'\r', b'\n', b'$', 7];
    client.set(&bin, &raw).await.expect("SET");
    assert_eq!(client.get(&bin).await.expect("GET"), Some(raw), "двоичное значение");
    assert_eq!((client.incr(&count).await.expect("INCR"), client.incr(&count).await.expect("INCR")), (1, 2), "счётчик");
    assert!(client.del(&count).await.expect("DEL"), "ключ был, DEL true");
    assert!(!client.del(&count).await.expect("DEL"), "ключа уже нет, DEL false");
    client.set_ex(&tmp, b"x", 100).await.expect("SET EX");
    let ttl = client.ttl(&tmp).await.expect("TTL");
    assert!((90..=100).contains(&ttl), "срок должен быть около 100 секунд, а TTL вернул {ttl}");
    assert!(client.expire(&name, 50).await.expect("EXPIRE"), "EXPIRE существующего ключа");
    assert_eq!(client.ttl(&scope.key("none")).await.expect("TTL"), -2, "нет ключа: -2");
}

#[tokio::test]
async fn errors_are_typed() {
    let scope = Scope::new();
    let client = Client::connect(&redis_url()).await.expect("подключение");
    let list = scope.key("list");
    client.call(&["RPUSH", list.as_str(), "a"]).await.expect("RPUSH");
    match client.get(&list).await {
        Err(Error::Server { kind, .. }) => assert_eq!(kind, "WRONGTYPE", "вид ошибки сервера"),
        other => panic!("GET по списку: ожидалась Error::Server, а получено {other:?}"),
    }
    let server = Fake::start(|_, cmd| if cmd[0] == "GET" { say(":5\r\n") } else { say("+PONG\r\n") }).await;
    let fake = Client::connect(&server.url()).await.expect("подключение к фальшивому");
    assert!(matches!(fake.get("k").await, Err(Error::Unexpected(Value::Int(5)))), "число в ответ на GET это Error::Unexpected");
    assert_eq!(fake.call(&["PING"]).await.expect("PING"), Value::Simple("PONG".into()), "после ошибки типа соединение живо");
}

#[tokio::test]
async fn pipeline_keeps_order_and_isolates_errors() {
    let scope = Scope::new();
    let client = Client::connect(&redis_url()).await.expect("подключение");
    let k = scope.key("k");
    let got = client.pipeline(&[cmd(&["SET", &k, "text"]), cmd(&["INCR", &k]), cmd(&["GET", &k])]).await.expect("pipeline");
    assert_eq!(got[0], Value::Simple("OK".into()), "первый ответ");
    assert!(matches!(&got[1], Value::Error(text) if text.contains("not an integer")), "ошибка команды остаётся значением внутри пачки: {:?}", got[1]);
    assert_eq!(got[2], Value::Bulk(b"text".to_vec()), "третий ответ на месте");
    assert_eq!(client.call(&["PING"]).await.expect("PING"), Value::Simple("PONG".into()), "после пачки с ошибкой соединение не рассинхронизировано");
    let many: Vec<Vec<String>> = (0..2000).map(|i| cmd(&["SET", &scope.key(&format!("m{i}")), &i.to_string()])).collect();
    assert_eq!(client.pipeline(&many).await.expect("большая пачка").len(), 2000, "по ответу на команду");
}

#[tokio::test]
async fn fifty_tasks_share_one_client() {
    let scope = Scope::new();
    let client = Client::connect(&redis_url()).await.expect("подключение");
    let key = scope.key("n");
    let mut tasks = Vec::new();
    for _ in 0..50 {
        let (client, key) = (client.clone(), key.clone());
        tasks.push(tokio::spawn(async move {
            let mut seen = Vec::new();
            for _ in 0..20 {
                seen.push(client.incr(&key).await.expect("INCR"));
            }
            seen
        }));
    }
    let mut all = Vec::new();
    for t in tasks {
        all.extend(t.await.expect("задача"));
    }
    all.sort_unstable();
    assert_eq!(all, (1..=1000).collect::<Vec<i64>>(), "каждый вызов получает свой ответ");
}

#[tokio::test]
async fn timeout_does_not_desync() {
    let server = Fake::start(|_, cmd| match cmd[0].as_str() {
        "SLOW" => vec![Step::Sleep(Duration::from_millis(400)), Step::Write(b"+late\r\n".to_vec())],
        _ => say("+PONG\r\n"),
    })
    .await;
    let client = Client::connect_with(&server.url(), quick()).await.expect("подключение");
    assert!(matches!(client.call(&["SLOW"]).await, Err(Error::Timeout)), "медленная команда это Error::Timeout");
    sleep(Duration::from_millis(500)).await;
    assert_eq!(client.call(&["PING"]).await.expect("PING"), Value::Simple("PONG".into()), "запоздавший ответ не должен достаться следующей команде");
    assert_eq!(server.connections(), 1, "срок не требует нового соединения");
}

#[tokio::test]
async fn reconnects_and_fails_what_was_in_flight() {
    let server = Fake::start(|_, cmd| match cmd[0].as_str() {
        "DIE" => vec![Step::Close],
        "HANG" => Vec::new(),
        _ => say("+PONG\r\n"),
    })
    .await;
    let options = Options { command_timeout: Duration::from_secs(2), ..quick() };
    let client = Client::connect_with(&server.url(), options).await.expect("подключение");
    let hanging = {
        let client = client.clone();
        tokio::spawn(async move { client.call(&["HANG"]).await })
    };
    sleep(Duration::from_millis(80)).await;
    assert!(matches!(client.call(&["DIE"]).await, Err(Error::Disconnected)), "команда, на которой оборвалось соединение, получает Error::Disconnected");
    let hung = timeout(Duration::from_secs(2), hanging).await.expect("запрос в оборванном соединении завис").expect("задача");
    assert!(matches!(hung, Err(Error::Disconnected)), "запрос, ушедший в оборванное соединение, получает Error::Disconnected, а не повтор");
    let pong = timeout(Duration::from_secs(3), client.call(&["PING"])).await.expect("после обрыва клиент не вернулся").expect("PING");
    assert_eq!(pong, Value::Simple("PONG".into()), "после переподключения всё работает");
    assert_eq!(server.connections(), 2, "второе соединение, не больше");
}

#[tokio::test]
async fn hostile_length_header_is_a_protocol_error() {
    let server = Fake::start(|_, cmd| if cmd[0] == "BIG" { say("$999999999999\r\n") } else { say("+PONG\r\n") }).await;
    let client = Client::connect_with(&server.url(), Options { command_timeout: Duration::from_secs(2), ..quick() }).await.expect("подключение");
    match timeout(Duration::from_secs(2), client.call(&["BIG"])).await.expect("клиент ждёт тело, которое не придёт: предел надо проверять по заголовку") {
        Err(Error::Protocol(_)) => {}
        other => panic!("ожидалась Error::Protocol, а получено {other:?}"),
    }
    let pong = timeout(Duration::from_secs(3), client.call(&["PING"])).await.expect("завис").expect("PING");
    assert_eq!(pong, Value::Simple("PONG".into()), "после нарушения протокола клиент открывает новое соединение");
    assert_eq!(server.connections(), 2, "испорченное соединение закрыто");
}

#[tokio::test]
async fn blocking_pop_does_not_stall_the_others() {
    let scope = Scope::new();
    let client = Client::connect(&redis_url()).await.expect("подключение");
    let list = scope.key("jobs");
    let waiter = {
        let (client, list) = (client.clone(), list.clone());
        tokio::spawn(async move { client.blpop(&list, 3).await })
    };
    sleep(Duration::from_millis(150)).await;
    let pong = timeout(Duration::from_millis(600), client.call(&["PING"])).await.expect("PING ждёт, пока кончится BLPOP").expect("PING");
    assert_eq!(pong, Value::Simple("PONG".into()), "обычная команда во время BLPOP");
    client.call(&["RPUSH", list.as_str(), "job"]).await.expect("RPUSH");
    let got = timeout(Duration::from_secs(2), waiter).await.expect("BLPOP не проснулся").expect("задача").expect("BLPOP");
    assert_eq!(got, Some((list, b"job".to_vec())), "воркер получил задачу");
    let empty = timeout(Duration::from_secs(4), client.blpop(&scope.key("empty"), 1)).await.expect("завис").expect("BLPOP");
    assert_eq!(empty, None, "пустой список: по сроку None");
}

#[tokio::test]
async fn in_flight_is_bounded() {
    let server = Fake::start(|_, cmd| if cmd[0] == "PING" { say("+PONG\r\n") } else { Vec::new() }).await;
    let options = Options { max_in_flight: 2, command_timeout: Duration::from_millis(300), ..quick() };
    let client = Client::connect_with(&server.url(), options).await.expect("подключение");
    let mut tasks = Vec::new();
    for _ in 0..6 {
        let client = client.clone();
        tasks.push(tokio::spawn(async move { client.call(&["X"]).await }));
    }
    for t in tasks {
        assert!(matches!(t.await.expect("задача"), Err(Error::Timeout)), "сервер молчит, все вызовы кончаются сроком");
    }
    let sent = server.commands().iter().filter(|c| c[0] == "X").count();
    assert_eq!(sent, 2, "в полёте не больше двух команд, а до сервера дошло {sent}");
}

#[tokio::test]
async fn dropping_every_copy_closes_the_connection() {
    let server = Fake::start(|_, _| say("+PONG\r\n")).await;
    let client = Client::connect(&server.url()).await.expect("подключение");
    let copy = client.clone();
    copy.call(&["PING"]).await.expect("PING");
    drop(client);
    drop(copy);
    for _ in 0..50 {
        if server.open() == 0 {
            return;
        }
        sleep(Duration::from_millis(40)).await;
    }
    panic!("все копии клиента брошены, а соединение осталось открытым");
}
