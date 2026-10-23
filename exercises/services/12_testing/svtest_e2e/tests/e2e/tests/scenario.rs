use async_nats::Client;
use stack::Stack;
use std::net::SocketAddr;
use std::time::Duration;
use testkit::{eventually, unique_prefix};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::net::TcpStream;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::time::timeout;

const NATS: &str = "nats://127.0.0.1:54222";

async fn broker() -> Client {
    let url = std::env::var("RUSTLINGS_NATS_URL").unwrap_or_else(|_| NATS.to_string());
    async_nats::connect(&url).await.unwrap_or_else(|e| panic!("NATS не отвечает по адресу {url}: {e}. Запусти `docker compose up -d`."))
}

struct Conn {
    lines: Lines<BufReader<OwnedReadHalf>>,
    write: OwnedWriteHalf,
}

impl Conn {
    async fn open(addr: SocketAddr) -> Conn {
        let stream = timeout(Duration::from_secs(2), TcpStream::connect(addr)).await.expect("подключение за две секунды").expect("шлюз принимает соединения");
        let (read, write) = stream.into_split();
        Conn { lines: BufReader::new(read).lines(), write }
    }

    async fn say(&mut self, line: &str) -> String {
        self.write.write_all(format!("{line}\n").as_bytes()).await.expect("запись в шлюз");
        self.next_line(Duration::from_secs(3)).await.unwrap_or_else(|| panic!("шлюз не ответил на {line:?} за три секунды"))
    }

    async fn next_line(&mut self, within: Duration) -> Option<String> {
        timeout(within, self.lines.next_line()).await.ok()?.ok()?
    }
}

async fn token_of(addr: SocketAddr, name: &str) -> (Conn, String) {
    let mut conn = Conn::open(addr).await;
    assert_eq!(conn.say(&format!("REGISTER {name} secret")).await, "OK", "регистрация {name}");
    let reply = conn.say(&format!("LOGIN {name} secret")).await;
    let token = reply.strip_prefix("TOKEN ").unwrap_or_else(|| panic!("вход {name}: {reply}")).to_string();
    (conn, token)
}

async fn story(client: &Client) -> (Stack, Vec<String>) {
    let stack = Stack::start(client, &unique_prefix()).await;
    let (mut ann, ann_token) = token_of(stack.addr, "ann").await;
    let (mut bob, bob_token) = token_of(stack.addr, "bob").await;
    assert_eq!(bob.say(&format!("FOLLOW {bob_token} ann")).await, "OK", "Боб подписывается на Анну");
    assert_eq!(bob.say(&format!("LISTEN {bob_token}")).await, "OK", "Боб слушает ленту");
    assert_eq!(ann.say(&format!("POST {ann_token} hello world")).await, "OK", "Анна пишет пост");
    let pushed = bob.next_line(Duration::from_secs(5)).await.expect("Боб получил push о посте Анны");
    let addr = stack.addr;
    let unread = eventually(Duration::from_secs(5), Duration::from_millis(25), || {
        let token = bob_token.clone();
        async move {
            let reply = Conn::open(addr).await.say(&format!("UNREAD {token}")).await;
            if reply == "UNREAD 1" { Ok(reply) } else { Err(format!("счётчик непрочитанных: {reply}")) }
        }
    })
    .await;
    let mut seen = vec![pushed];
    seen.push(unread.expect("счётчик непрочитанных дорос до единицы"));
    (stack, seen)
}

#[tokio::test]
async fn register_login_follow_post_and_push_work_end_to_end() {
    let client = broker().await;
    let (stack, seen) = story(&client).await;
    assert_eq!(seen[0], "NEW ann hello world", "Боб увидел пост Анны в виде NEW <автор> <текст>");
    assert_eq!(seen[1], "UNREAD 1", "и непрочитанный один");
    stack.stop().await;
}

#[tokio::test]
async fn the_first_request_right_after_start_works() {
    let client = broker().await;
    let began = std::time::Instant::now();
    let stack = Stack::start(&client, &unique_prefix()).await;
    let mut conn = Conn::open(stack.addr).await;
    assert_eq!(conn.say("REGISTER zed pw").await, "OK", "первая же команда сразу после start должна пройти: все сервисы к этому моменту готовы");
    assert!(began.elapsed() < Duration::from_secs(3), "поднялось и ответило за {:?}: без sleep и ожиданий", began.elapsed());
    stack.stop().await;
}

#[tokio::test]
async fn two_stacks_in_one_process_do_not_hear_each_other() {
    let client = broker().await;
    let (first, second) = tokio::join!(story(&client), story(&client));
    assert_eq!(first.1[0], "NEW ann hello world", "первый стек: push на месте");
    assert_eq!(second.1[0], "NEW ann hello world", "второй стек: тоже, и ровно один, а не два");
    assert_ne!(first.0.prefix, second.0.prefix, "у стеков разные префиксы");
    assert_ne!(first.0.addr, second.0.addr, "и разные порты");
    first.0.stop().await;
    second.0.stop().await;
}

#[tokio::test]
async fn a_bad_token_publishes_nothing() {
    let client = broker().await;
    let stack = Stack::start(&client, &unique_prefix()).await;
    let (mut ann, ann_token) = token_of(stack.addr, "ann").await;
    let (mut bob, bob_token) = token_of(stack.addr, "bob").await;
    bob.say(&format!("FOLLOW {bob_token} ann")).await;
    assert_eq!(bob.say(&format!("LISTEN {bob_token}")).await, "OK", "Боб слушает");
    assert_eq!(ann.say("POST tok-forged hello").await, "ERR unauthorized", "чужой токен не пускает");
    assert!(bob.next_line(Duration::from_millis(400)).await.is_none(), "Боб ничего не получил: поддельный пост не должен попасть в события");
    assert_eq!(ann.say(&format!("POST {ann_token} real")).await, "OK", "настоящий пост проходит");
    assert_eq!(bob.next_line(Duration::from_secs(5)).await.as_deref(), Some("NEW ann real"), "и доходит");
    stack.stop().await;
}

#[tokio::test]
async fn stop_leaves_no_tasks_no_port_and_no_subscribers() {
    let client = broker().await;
    let prefix = unique_prefix();
    let stack = Stack::start(&client, &prefix).await;
    let addr = stack.addr;
    let mut idle = Conn::open(addr).await;
    assert_eq!(idle.say("REGISTER yan pw").await, "OK", "стек жив");
    assert!(stack.live_tasks() >= 5, "работают задачи сервисов и обработчик соединения: {}", stack.live_tasks());
    let live_probe = stack.live_tasks();
    assert!(live_probe > 0, "задачи видны");
    timeout(Duration::from_secs(10), stack.stop()).await.expect("stop не завис, хотя у клиента открыто молчащее соединение");
    assert!(TcpStream::connect(addr).await.is_err(), "порт шлюза закрыт");
    assert!(idle.next_line(Duration::from_secs(2)).await.is_none(), "молчавший клиент получил конец потока");
    let answer = timeout(Duration::from_secs(2), client.request(format!("{prefix}.auth.register"), "{}".into())).await.expect("брокер ответил");
    assert!(answer.is_err(), "на теме auth никто не слушает: подписки сервисов сняты");
}
