use ntsv_chat::run;
use std::io;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio::time::timeout;

const WAIT: Duration = Duration::from_secs(3);

struct Server {
    addr: SocketAddr,
    stop: Option<oneshot::Sender<()>>,
    task: JoinHandle<io::Result<()>>,
}

async fn start(max: usize) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = oneshot::channel::<()>();
    let task = tokio::spawn(run(listener, max, async move {
        let _ = rx.await;
    }));
    Server { addr, stop: Some(tx), task }
}

struct Client {
    reader: BufReader<OwnedReadHalf>,
    writer: OwnedWriteHalf,
}

impl Client {
    async fn connect(addr: SocketAddr) -> Client {
        let (r, w) = TcpStream::connect(addr).await.unwrap().into_split();
        Client { reader: BufReader::new(r), writer: w }
    }

    async fn join(addr: SocketAddr, nick: &str) -> Client {
        let mut c = Client::connect(addr).await;
        c.send(&format!("NICK {nick}")).await;
        assert_eq!(c.line().await, "OK", "ник {nick} свободен, ждали OK");
        c
    }

    async fn send(&mut self, line: &str) {
        self.writer.write_all(format!("{line}\n").as_bytes()).await.unwrap();
    }

    async fn line(&mut self) -> String {
        self.maybe_line(WAIT).await.expect("сервер молчит: ждали строку, а за 3 секунды ничего не пришло")
    }

    async fn maybe_line(&mut self, wait: Duration) -> Option<String> {
        let mut s = String::new();
        match timeout(wait, self.reader.read_line(&mut s)).await {
            Ok(Ok(0)) => panic!("сервер закрыл соединение, хотя ждали строку"),
            Ok(Ok(_)) => Some(s.trim_end_matches('\n').to_owned()),
            Ok(Err(e)) => panic!("ошибка чтения: {e}"),
            Err(_) => None,
        }
    }

    async fn rest(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        loop {
            let mut s = String::new();
            let n = timeout(WAIT, self.reader.read_line(&mut s)).await.expect("соединение так и не закрылось").unwrap_or(0);
            if n == 0 {
                return out;
            }
            out.push(s.trim_end_matches('\n').to_owned());
        }
    }
}

#[tokio::test]
async fn two_clients_talk() {
    let server = start(10).await;
    let mut alice = Client::join(server.addr, "alice").await;
    let mut bob = Client::join(server.addr, "bob").await;
    assert_eq!(alice.line().await, "JOIN bob", "alice должна узнать, что вошёл bob");
    bob.send("MSG hello there").await;
    assert_eq!(alice.line().await, "FROM bob hello there", "сообщение должно дойти до alice в формате FROM <ник> <текст>");
    alice.send("MSG hi bob").await;
    assert_eq!(bob.line().await, "FROM alice hi bob", "ответ должен дойти до bob первой строкой: свои сообщения отправителю не возвращаются");
}

#[tokio::test]
async fn sender_does_not_get_own_message() {
    let server = start(10).await;
    let mut alice = Client::join(server.addr, "alice").await;
    alice.send("MSG anyone?").await;
    assert!(alice.maybe_line(Duration::from_millis(300)).await.is_none(), "отправитель получил своё же сообщение");
}

#[tokio::test]
async fn nick_rules() {
    let server = start(10).await;
    let _alice = Client::join(server.addr, "alice").await;
    let mut other = Client::connect(server.addr).await;
    other.send("NICK alice").await;
    assert_eq!(other.line().await, "ERR taken", "ник уже занят");
    other.send("NICK bad nick").await;
    assert_eq!(other.line().await, "ERR bad_nick", "пробел в нике недопустим");
    other.send("NICK ").await;
    assert_eq!(other.line().await, "ERR bad_nick", "пустой ник недопустим");
    other.send("NICK abcdefghijklmnopq").await;
    assert_eq!(other.line().await, "ERR bad_nick", "ник длиннее 16 символов недопустим");
    other.send("NICK bob").await;
    assert_eq!(other.line().await, "OK", "после неудачных попыток можно выбрать нормальный ник");
    other.send("NICK bob2").await;
    assert_eq!(other.line().await, "ERR already_named", "сменить ник нельзя");
}

#[tokio::test]
async fn commands_need_a_nick_and_unknown_is_an_error() {
    let server = start(10).await;
    let mut c = Client::connect(server.addr).await;
    c.send("MSG hi").await;
    assert_eq!(c.line().await, "ERR nick_required", "писать без ника нельзя");
    c.send("DANCE").await;
    assert_eq!(c.line().await, "ERR unknown", "неизвестная команда");
}

#[tokio::test]
async fn quit_says_bye_and_frees_the_nick() {
    let server = start(10).await;
    let mut alice = Client::join(server.addr, "alice").await;
    let mut bob = Client::join(server.addr, "bob").await;
    assert_eq!(alice.line().await, "JOIN bob");
    bob.send("QUIT").await;
    assert_eq!(bob.rest().await, vec!["BYE"], "после QUIT ждём BYE и закрытие");
    assert_eq!(alice.line().await, "LEAVE bob", "alice должна узнать, что bob ушёл");
    let _bob_again = Client::join(server.addr, "bob").await;
}

#[tokio::test]
async fn dropped_connection_is_cleaned_up() {
    let server = start(10).await;
    let mut alice = Client::join(server.addr, "alice").await;
    let bob = Client::join(server.addr, "bob").await;
    assert_eq!(alice.line().await, "JOIN bob");
    drop(bob);
    assert_eq!(alice.line().await, "LEAVE bob", "оборвавшийся клиент должен быть убран из комнаты");
    let _bob_again = Client::join(server.addr, "bob").await;
}

#[tokio::test]
async fn limit_makes_extra_clients_wait() {
    let server = start(2).await;
    let mut c1 = Client::join(server.addr, "c1").await;
    let _c2 = Client::join(server.addr, "c2").await;
    assert_eq!(c1.line().await, "JOIN c2");
    let mut c3 = Client::connect(server.addr).await;
    c3.send("NICK c3").await;
    assert!(
        c3.maybe_line(Duration::from_millis(400)).await.is_none(),
        "лимит 2 соединения, а третий уже получил ответ: разрешение надо брать до accept"
    );
    c1.send("QUIT").await;
    assert_eq!(c1.rest().await, vec!["BYE"]);
    assert_eq!(c3.line().await, "OK", "место освободилось, третий должен быть обслужен");
}

#[tokio::test]
async fn busy_room_keeps_lines_intact_and_ordered() {
    let server = start(10).await;
    let mut listener = Client::join(server.addr, "listener").await;
    let mut senders = Vec::new();
    for i in 0..3 {
        let mut c = Client::join(server.addr, &format!("s{i}")).await;
        senders.push(tokio::spawn(async move {
            for k in 0..50 {
                c.send(&format!("MSG {i}-{k}")).await;
            }
            c
        }));
    }
    let mut next = [0usize; 3];
    let mut got = 0;
    while got < 150 {
        let l = listener.line().await;
        if l.starts_with("JOIN ") {
            continue;
        }
        let rest = l.strip_prefix("FROM s").unwrap_or_else(|| panic!("строка «{l}» не в формате FROM <ник> <текст>"));
        let (who, text) = rest.split_once(' ').unwrap_or_else(|| panic!("строка «{l}» испорчена"));
        let who: usize = who.parse().unwrap();
        assert_eq!(text, format!("{who}-{}", next[who]), "сообщения отправителя s{who} пришли не по порядку или испорчены");
        next[who] += 1;
        got += 1;
    }
    for s in senders {
        s.await.unwrap();
    }
}

#[tokio::test]
async fn shutdown_says_bye_to_everyone() {
    let mut server = start(10).await;
    let mut a = Client::join(server.addr, "a").await;
    let mut b = Client::join(server.addr, "b").await;
    assert_eq!(a.line().await, "JOIN b");
    let _ = server.stop.take().unwrap().send(());
    assert_eq!(a.rest().await, vec!["BYE"], "при остановке клиент получает BYE и закрытие");
    assert_eq!(b.rest().await, vec!["BYE"], "при остановке клиент получает BYE и закрытие");
    let res = timeout(WAIT, server.task).await.expect("run не вернулась после остановки").unwrap();
    assert!(res.is_ok(), "штатная остановка должна давать Ok");
}

#[tokio::test]
async fn shutdown_without_clients() {
    let mut server = start(1).await;
    let _ = server.stop.take().unwrap().send(());
    timeout(WAIT, server.task).await.expect("run не вернулась").unwrap().unwrap();
    assert!(TcpStream::connect(server.addr).await.is_err(), "после остановки порт должен быть закрыт");
}
