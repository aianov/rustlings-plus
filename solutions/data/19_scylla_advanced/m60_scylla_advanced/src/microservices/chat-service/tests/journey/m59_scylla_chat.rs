use std::sync::Arc;

use chat_service::common::{Message, now_micros};
use chat_service::repository::{ChatPages, ChatRepository, ScyllaChats, bucket_of};
use contracts::{Cause, Origin, UserId};
use store_engine::{StoreConfig, connect, drop_keyspace};
use tokio::sync::mpsc;

const MONTH: u64 = 31 * 24 * 60 * 60 * 1_000_000;

fn letter(from: u64, to: u64, text: &str) -> Message {
    Message {
        from: UserId(from),
        to: UserId(to),
        text: text.to_string(),
        id: String::new(),
        edited: false,
    }
}

#[tokio::test]
async fn chat_history_pages_lists_and_exports() {
    let config = StoreConfig::from_env(&format!("t_chat_{}", std::process::id()));
    let _ = drop_keyspace(&config).await;
    let session = connect(&config)
        .await
        .expect("Scylla курса на 127.0.0.1:59042");
    let chats = Arc::new(ScyllaChats::open(session).await.expect("схема чата"));
    let (alice, bob, carol, dave) = (UserId(1), UserId(2), UserId(3), UserId(4));
    let now = now_micros();
    for n in 0..5 {
        chats
            .append_at(
                &letter(1, 2, &format!("старое {n}")),
                now - MONTH - 1_000 + n,
            )
            .await
            .expect("прошлый месяц");
    }
    assert_ne!(
        bucket_of(now - MONTH),
        bucket_of(now),
        "сообщения в разных бакетах"
    );
    for n in 0..20 {
        let from = if n % 2 == 0 { 1 } else { 2 };
        let to = 3 - from;
        let cause = Cause::new(&Origin::new("m59"), now + n);
        chats
            .append(letter(from, to, &format!("сообщение {n}")), &cause)
            .await
            .expect("отправка");
    }
    chats
        .append(
            letter(1, 3, "привет, carol"),
            &Cause::new(&Origin::new("m59"), now + 100),
        )
        .await
        .expect("carol");
    chats
        .append(
            letter(4, 1, "это dave"),
            &Cause::new(&Origin::new("m59"), now + 200),
        )
        .await
        .expect("dave");
    let mut seen = Vec::new();
    let mut cursor = None;
    loop {
        let (page, next) = chats
            .page(alice, bob, cursor.clone(), 10)
            .await
            .expect("страница");
        seen.extend(page.into_iter().map(|message| message.text));
        match next {
            Some(next) if seen.len() < 25 => cursor = Some(next),
            _ => break,
        }
    }
    assert_eq!(seen.len(), 25, "все сообщения через два бакета: {seen:?}");
    assert_eq!(seen[0], "сообщение 19", "новые первыми");
    assert_eq!(seen[24], "старое 0");
    let latest = chats.history(alice, bob, 3).await;
    let latest: Vec<&str> = latest.iter().map(|message| message.text.as_str()).collect();
    assert_eq!(
        latest,
        ["сообщение 17", "сообщение 18", "сообщение 19"],
        "старые первыми, как раньше"
    );
    let list = chats.chats(alice, 10).await.expect("чаты");
    let peers: Vec<UserId> = list.iter().map(|chat| chat.peer).collect();
    assert_eq!(
        peers,
        [dave, carol, bob],
        "свежие первыми, старые строки удалены"
    );
    assert_eq!(list[0].preview, "это dave");
    let broken = chats
        .page(alice, bob, Some(String::from("кривой")), 10)
        .await;
    assert!(broken.is_err(), "курсор проверяется");
    let (sink, mut rows) = mpsc::channel(8);
    let reading = async {
        let mut count = 0;
        while rows.recv().await.is_some() {
            count += 1;
        }
        count
    };
    let (sent, count) = tokio::join!(chats.export(alice, bob, sink), reading);
    let sent = sent.expect("выгрузка");
    assert_eq!((sent, count), (25, 25), "execute_iter прошёл все бакеты");
    drop_keyspace(&config).await.expect("keyspace убран");
}
