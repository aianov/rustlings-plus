use ntx_bidi::pb::chat_client::ChatClient;
use ntx_bidi::pb::chat_in::Kind;
use ntx_bidi::pb::{ChatIn, ChatOut, Join, Say};
use ntx_bidi::{start, Room};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::{Channel, Endpoint};
use tonic::{Code, Streaming};

async fn within<T>(what: &str, f: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(std::time::Duration::from_secs(5), f)
        .await
        .unwrap_or_else(|_| panic!("{what}: не дождались ответа за 5 секунд"))
}

async fn setup() -> ChatClient<Channel> {
    let addr = start(Room::new()).await.unwrap();
    ChatClient::new(Endpoint::from_shared(format!("http://{addr}")).unwrap().connect_lazy())
}

fn join(name: &str) -> ChatIn {
    ChatIn { kind: Some(Kind::Join(Join { name: name.to_string() })) }
}

fn say(text: &str) -> ChatIn {
    ChatIn { kind: Some(Kind::Say(Say { text: text.to_string() })) }
}

struct Member {
    tx: mpsc::Sender<ChatIn>,
    rx: Streaming<ChatOut>,
}

async fn enter(client: &mut ChatClient<Channel>, name: &str) -> Member {
    let (tx, rx) = mpsc::channel(8);
    tx.send(join(name)).await.unwrap();
    let rx = within("вход в чат", client.talk(ReceiverStream::new(rx))).await.unwrap().into_inner();
    Member { tx, rx }
}

impl Member {
    async fn next(&mut self) -> (String, String) {
        let m = within("ожидание сообщения", self.rx.message()).await.unwrap().expect("поток ответов закрылся раньше времени");
        (m.from, m.text)
    }
}

#[tokio::test]
async fn a_silent_listener_hears_everything_and_the_author_hears_himself() {
    let mut client = setup().await;
    let mut ann = enter(&mut client, "ann").await;
    let mut bob = enter(&mut client, "bob").await;
    for text in ["раз", "два", "три"] {
        ann.tx.send(say(text)).await.unwrap();
    }
    for text in ["раз", "два", "три"] {
        assert_eq!(bob.next().await, ("ann".to_string(), text.to_string()), "молчащий участник должен получать чужие сообщения: входящий и исходящий потоки независимы");
        assert_eq!(ann.next().await, ("ann".to_string(), text.to_string()), "автор тоже получает своё сообщение");
    }
}

#[tokio::test]
async fn latecomer_does_not_see_the_past() {
    let mut client = setup().await;
    let mut ann = enter(&mut client, "ann").await;
    ann.tx.send(say("старое")).await.unwrap();
    assert_eq!(ann.next().await.1, "старое");
    let mut bob = enter(&mut client, "bob").await;
    ann.tx.send(say("новое")).await.unwrap();
    assert_eq!(bob.next().await.1, "новое", "опоздавший должен начать с первого сообщения после своего входа");
}

#[tokio::test]
async fn bad_join_is_invalid_argument() {
    let mut client = setup().await;
    let cases: Vec<(&str, Vec<ChatIn>)> = vec![
        ("пустой поток", vec![]),
        ("первое сообщение Say", vec![say("привет")]),
        ("пустое имя", vec![join("   ")]),
        ("слишком длинное имя", vec![join(&"x".repeat(21))]),
        ("ChatIn без содержимого", vec![ChatIn { kind: None }]),
    ];
    for (what, messages) in cases {
        let (tx, rx) = mpsc::channel(8);
        for m in messages {
            tx.send(m).await.unwrap();
        }
        drop(tx);
        let err = within("talk", client.talk(ReceiverStream::new(rx))).await.expect_err(what);
        assert_eq!(err.code(), Code::InvalidArgument, "{what}: ждали InvalidArgument, получили {err}");
    }
}

#[tokio::test]
async fn closing_the_send_side_ends_only_that_members_stream() {
    let mut client = setup().await;
    let mut ann = enter(&mut client, "ann").await;
    let mut bob = enter(&mut client, "bob").await;
    let carol = enter(&mut client, "carol").await;
    drop(ann.tx);
    let end = within("конец потока", ann.rx.message()).await.unwrap();
    assert!(end.is_none(), "клиент закрыл отправку, его поток ответов должен закончиться");
    carol.tx.send(say("всем привет")).await.unwrap();
    assert_eq!(bob.next().await, ("carol".to_string(), "всем привет".to_string()), "уход одного участника не должен ломать остальных");
    tokio::time::sleep(Duration::from_millis(10)).await;
}
