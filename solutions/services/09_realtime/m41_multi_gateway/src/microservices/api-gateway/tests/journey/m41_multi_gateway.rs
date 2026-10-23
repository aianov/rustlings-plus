use std::time::Duration;

use api_gateway::Config;
use async_nats::RequestErrorKind;
use contracts::{Changed, MessageView, OnlineView, Push, TypingNote, UserId};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, Lines};

use crate::support::{Gateway, Peer, letter, nats_url, ok, plain, within};

const QUIET: Duration = Duration::from_millis(300);

const TTL: Duration = Duration::from_millis(1000);

fn short_presence() -> Config {
    Config {
        presence_ttl: TTL,
        ..Config::default()
    }
}

fn seq_of(push: &Push) -> u64 {
    match push {
        Push::Message { seq, .. } => *seq,
        other => panic!("ждали сообщение, а пришло {other:?}"),
    }
}

fn say(peer: UserId, text: &str) -> String {
    format!(r#"{{"peer": "{peer}", "text": "{text}"}}"#)
}

async fn online(asker: &mut Peer, user: UserId) -> bool {
    ok!(asker, "users.online", format!(r#"{{"target": "{user}"}}"#) => OnlineView).online
}

async fn online_becomes(asker: &mut Peer, user: UserId, want: bool, what: &str) {
    within(what, async {
        while online(asker, user).await != want {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await;
}

async fn nobody_listens(subject: &str) -> bool {
    let client = async_nats::connect(nats_url()).await.expect("NATS курса");
    let asked = client.request(subject.to_string(), "?".into());
    match tokio::time::timeout(QUIET, asked).await {
        Ok(Err(error)) => error.kind() == RequestErrorKind::NoResponders,
        Ok(Ok(..)) | Err(..) => false,
    }
}

#[tokio::test]
async fn a_message_reaches_every_device_on_every_gateway_once() {
    let stack = Gateway::start(Config::default()).await;
    let twin = stack.twin(Config::default()).await;
    let alice = stack.register("alice").await;
    let bob = stack.register("bob").await;
    let mut phone = stack.device("alice").await;
    let mut laptop = stack.device("alice").await;
    let mut tablet = twin.peer_with(&stack.login("alice").await.token).await;
    let mut sender = twin.peer_with(&stack.login("bob").await.token).await;
    let sent = ok!(sender, "chat.send", say(alice.id, "привет с другого шлюза") => MessageView);
    assert_eq!((sent.from, sent.to), (bob.id, alice.id));
    let mut seqs = Vec::new();
    for (name, device) in [
        ("телефон на шлюзе A", &mut phone),
        ("ноутбук на шлюзе A", &mut laptop),
        ("планшет на шлюзе B", &mut tablet),
    ] {
        let push = device.push().await;
        seqs.push(seq_of(&push));
        assert_eq!(
            plain(push),
            letter(alice.id, bob.id, "привет с другого шлюза"),
            "{name}: сообщение пришло с текстом, на каждом шлюзе, где есть устройства alice"
        );
        assert_eq!(
            device.quiet(QUIET).await,
            None,
            "{name}: ровно один раз, хотя устройств на шлюзе A два"
        );
    }
    assert!(
        seqs.iter().all(|seq| *seq > 0 && *seq == seqs[0]),
        "у сообщения один номер в stream на всех устройствах: {seqs:?}"
    );
    assert_eq!(
        sender.quiet(QUIET).await,
        None,
        "отправителю его же сообщение не приходит"
    );
    twin.stop().await;
    stack.stop().await;
}

#[tokio::test]
async fn one_subscription_per_user_per_gateway_and_none_after_the_last_device() {
    let stack = Gateway::start(Config::default()).await;
    let alice = stack.register("alice").await;
    let mut phone = stack.device("alice").await;
    let mut laptop = stack.device("alice").await;
    let subject = stack.topics.typing(alice.id).join(".");
    let note = TypingNote {
        from: UserId(77),
        to: alice.id,
    };
    let client = async_nats::connect(nats_url()).await.expect("NATS курса");
    let body = contracts::encode(&note).expect("JSON");
    client
        .publish(subject.clone(), body.into())
        .await
        .expect("публикация");
    client.flush().await.expect("flush");
    for device in [&mut phone, &mut laptop] {
        assert_eq!(
            device.push().await,
            Push::Typing {
                to: alice.id,
                from: UserId(77)
            }
        );
        assert_eq!(
            device.quiet(QUIET).await,
            None,
            "подписка на шлюзе одна на пользователя: подписка на каждое соединение дала бы по две копии"
        );
    }
    drop(phone);
    tokio::time::sleep(QUIET).await;
    assert!(
        !nobody_listens(&subject).await,
        "у alice ещё есть устройство: подписка шлюза жива"
    );
    drop(laptop);
    within(
        "подписка снята с последним устройством",
        async {
            while !nobody_listens(&subject).await {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        },
    )
    .await;
    stack.stop().await;
}

#[tokio::test]
async fn typing_reaches_the_peer_paced_and_unstored() {
    let stack = Gateway::start(Config::default()).await;
    let twin = stack.twin(Config::default()).await;
    let alice = stack.register("alice").await;
    let bob = stack.register("bob").await;
    let mut reader = stack.device("alice").await;
    let mut writer = twin.peer_with(&stack.login("bob").await.token).await;
    let typing = format!(r#"{{"peer": "{}"}}"#, alice.id);
    let first = ok!(writer, "chat.typing", typing.clone() => Changed);
    assert!(first.changed, "первое «печатает» уходит");
    assert_eq!(
        reader.push().await,
        Push::Typing {
            to: alice.id,
            from: bob.id
        },
        "alice на другом шлюзе видит, что bob печатает"
    );
    let second = ok!(writer, "chat.typing", typing => Changed);
    assert!(
        !second.changed,
        "второе за 3 секунды не уходит: «печатает» не шлётся на каждое нажатие"
    );
    assert_eq!(reader.quiet(QUIET).await, None);
    let own = writer
        .fails("chat.typing", &format!(r#"{{"peer": "{}"}}"#, bob.id))
        .await;
    assert_eq!(own.code, "rejected", "себе не печатают: {own:?}");
    let js = async_nats::jetstream::new(async_nats::connect(nats_url()).await.expect("NATS"));
    let mut chat = js
        .get_stream(stack.topics.chat_stream())
        .await
        .expect("stream чата заведён");
    let stored = chat.info().await.expect("сведения stream").state.messages;
    assert_eq!(stored, 0, "«печатает» эфемерно и в stream чата не попадает");
    twin.stop().await;
    stack.stop().await;
}

#[tokio::test]
async fn presence_lives_while_any_device_lives() {
    let stack = Gateway::start(short_presence()).await;
    let twin = stack.twin(short_presence()).await;
    let alice = stack.register("alice").await;
    let (_, mut asker) = stack.member("bob").await;
    assert!(!online(&mut asker, alice.id).await, "alice ещё не входила");
    let phone = stack.device("alice").await;
    let tablet = twin.peer_with(&stack.login("alice").await.token).await;
    assert!(online(&mut asker, alice.id).await, "alice в сети");
    drop(phone);
    tokio::time::sleep(QUIET).await;
    assert!(
        online(&mut asker, alice.id).await,
        "закрыт телефон, но планшет на другом шлюзе жив: alice в сети"
    );
    tokio::time::sleep(TTL * 2).await;
    assert!(
        online(&mut asker, alice.id).await,
        "дольше срока жизни записи alice в сети: шлюз B шлёт сигналы за планшет"
    );
    drop(tablet);
    online_becomes(
        &mut asker,
        alice.id,
        false,
        "alice не в сети после последнего устройства",
    )
    .await;
    twin.stop().await;
    stack.stop().await;
}

#[tokio::test]
async fn a_returning_device_catches_up_in_order_without_repeats() {
    let stack = Gateway::start(Config::default()).await;
    let twin = stack.twin(Config::default()).await;
    let alice = stack.register("alice").await;
    let bob = stack.register("bob").await;
    let mut sender = stack.device("bob").await;
    let token = stack.login("alice").await.token;
    let mut phone = stack.peer_with(&token).await;
    let _ = ok!(sender, "chat.send", say(alice.id, "m1") => MessageView);
    let seen = seq_of(&phone.push().await);
    drop(phone);
    for text in ["m2", "m3", "m4"] {
        let _ = ok!(sender, "chat.send", say(alice.id, text) => MessageView);
    }
    let (mut back, sent) = tokio::join!(twin.peer_after(&token, seen), async {
        tokio::time::sleep(Duration::from_millis(5)).await;
        let mut again = stack.device("bob").await;
        ok!(again, "chat.send", say(alice.id, "m5") => MessageView)
    });
    assert_eq!(sent.text, "m5");
    let mut texts = Vec::new();
    let mut seqs = vec![seen];
    for _ in 0..4 {
        let push = back.push().await;
        seqs.push(seq_of(&push));
        if let Push::Message { text, .. } = push {
            texts.push(text);
        }
    }
    assert_eq!(
        texts,
        ["m2", "m3", "m4", "m5"],
        "пропущенное приходит по порядку, а сообщение, отправленное во время догона, не теряется"
    );
    assert!(
        seqs.windows(2).all(|pair| pair[0] < pair[1]),
        "номера растут: {seqs:?}"
    );
    assert_eq!(
        back.quiet(QUIET).await,
        None,
        "ни одного повтора: история и живая подписка склеены по номеру"
    );
    let _ = ok!(sender, "chat.send", say(alice.id, "m6") => MessageView);
    assert_eq!(
        plain(back.push().await),
        letter(alice.id, bob.id, "m6"),
        "после догона живые сообщения идут как обычно"
    );
    let mut fresh = stack.peer_after(&stack.login("alice").await.token, 0).await;
    let mut all = Vec::new();
    for _ in 0..6 {
        if let Push::Message { text, .. } = fresh.push().await {
            all.push(text);
        }
    }
    assert_eq!(
        all,
        ["m1", "m2", "m3", "m4", "m5", "m6"],
        "новое устройство с after=0 получает всю переписку"
    );
    twin.stop().await;
    stack.stop().await;
}

#[tokio::test]
async fn logout_closes_the_session_on_every_gateway() {
    let stack = Gateway::start(Config::default()).await;
    let twin = stack.twin(Config::default()).await;
    stack.register("alice").await;
    let first = stack.login("alice").await.token;
    let second = stack.login("alice").await.token;
    let mut here = stack.peer_with(&first).await;
    let mut there = twin.peer_with(&first).await;
    let mut other = twin.peer_with(&second).await;
    let out = stack
        .web()
        .request("POST", "/auth/logout", None, Some(&first))
        .await
        .expect("выход");
    assert!(out.status < 300, "выход проходит: {out:?}");
    for (name, device) in [("шлюз A", &mut here), ("шлюз B", &mut there)] {
        assert!(device.closed().await, "{name}: соединение сессии закрыто");
        assert_eq!(
            device.close_code(),
            Some(4003),
            "{name}: код 4003, сессия отозвана"
        );
    }
    let _ = ok!(other, "users.find", r#"{"username": "alice"}"# => contracts::UserView);
    let refused = twin.refused(&format!("/ws?token={first}")).await;
    assert!(
        refused.contains("401"),
        "отозванная сессия не открывает соединение ни на одном шлюзе: {refused}"
    );
    let all = stack
        .web()
        .request("POST", "/auth/logout-all", None, Some(&second))
        .await
        .expect("выход везде");
    assert!(all.status < 300, "выход везде проходит: {all:?}");
    assert!(
        other.closed().await,
        "выход на всех устройствах закрывает и вторую сессию"
    );
    assert_eq!(other.close_code(), Some(4003));
    twin.stop().await;
    stack.stop().await;
}

async fn read_until(screen: &mut Lines<BufReader<DuplexStream>>, needle: &str) -> Vec<String> {
    let mut seen = Vec::new();
    loop {
        let line = within(&format!("строка клиента с «{needle}»"), screen.next_line())
            .await
            .expect("вывод клиента читается")
            .unwrap_or_else(|| panic!("клиент замолчал, так и не написав «{needle}»: {seen:?}"));
        let found = line.contains(needle);
        seen.push(line);
        if found {
            return seen;
        }
    }
}

struct Window {
    keys: DuplexStream,
    screen: Lines<BufReader<DuplexStream>>,
}

impl Window {
    fn open(addr: String) -> Window {
        let (keys, stdin) = tokio::io::duplex(4096);
        let (stdout, screen) = tokio::io::duplex(1 << 16);
        tokio::spawn(async move { social_cli::run(&addr, BufReader::new(stdin), stdout).await });
        Window {
            keys,
            screen: BufReader::new(screen).lines(),
        }
    }

    async fn type_line(&mut self, line: &str) {
        self.keys
            .write_all(format!("{line}\n").as_bytes())
            .await
            .expect("ввод пишется");
    }

    async fn sees(&mut self, needle: &str) -> Vec<String> {
        read_until(&mut self.screen, needle).await
    }
}

#[tokio::test]
async fn course_clients_on_two_gateways_talk_type_and_see_presence() {
    let stack = Gateway::start(Config::default()).await;
    let twin = stack.twin(Config::default()).await;
    stack.register("alice").await;
    stack.register("bob").await;
    let mut alice = Window::open(stack.http.to_string());
    let mut bob = Window::open(twin.http.to_string());
    alice.type_line("login alice correct-horse").await;
    alice.sees("привет, @alice").await;
    bob.type_line("login bob correct-horse").await;
    bob.sees("привет, @bob").await;
    bob.type_line("online alice").await;
    bob.sees("@alice в сети").await;
    bob.type_line("typing alice").await;
    bob.sees("видит «печатает…»").await;
    alice.sees("* @bob печатает…").await;
    bob.type_line("msg alice привет через два шлюза").await;
    alice.sees("* @bob: привет через два шлюза").await;
    alice.type_line("quit").await;
    bob.type_line("quit").await;
    twin.stop().await;
    stack.stop().await;
}
