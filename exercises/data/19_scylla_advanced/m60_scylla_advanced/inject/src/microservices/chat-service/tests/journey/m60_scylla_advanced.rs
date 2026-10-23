use std::time::Duration;

use chat_service::common::{Message, now_micros};
use chat_service::repository::{
    ChatExtras, ChatPages, ChatRepository, Edited, ScyllaChats, UNREAD_CAP, chat_of,
};
use contracts::UserId;
use store_engine::{StoreConfig, connect, drop_keyspace};

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
async fn chats_open_once_edits_keep_order_and_messages_expire() {
    let config = StoreConfig::from_env(&format!("t_chat_more_{}", std::process::id()));
    let _ = drop_keyspace(&config).await;
    let session = connect(&config)
        .await
        .expect("Scylla курса на 127.0.0.1:59042");
    let chats = ScyllaChats::open(session.clone()).await.expect("схема");
    let again = ScyllaChats::open(session.clone()).await;
    assert!(again.is_ok(), "ALTER TABLE ADD второй раз не ломает старт");
    let (alice, bob) = (UserId(1), UserId(2));
    let now = now_micros();
    let chat = chat_of(alice, bob);
    assert!(
        chats.open_chat(&chat, alice, bob, now).await.expect("LWT"),
        "первый раз создаётся"
    );
    assert!(
        !chats.open_chat(&chat, bob, alice, now).await.expect("LWT"),
        "IF NOT EXISTS: второй раз нет"
    );
    let sent = chats
        .append_at(&letter(1, 2, "первая версия"), now)
        .await
        .expect("отправка");
    assert!(!sent.id.is_empty(), "сообщение получило timeuuid");
    assert_eq!(
        chats
            .edit_at(alice, &sent.id, "новая правка", now + 2_000)
            .await
            .expect("правка"),
        Edited::Changed
    );
    assert_eq!(
        chats
            .edit_at(alice, &sent.id, "старая правка", now + 1_000)
            .await
            .expect("правка"),
        Edited::Changed
    );
    let shown = chats.history(alice, bob, 10).await;
    assert_eq!(
        shown.last().map(|message| message.text.as_str()),
        Some("новая правка"),
        "USING TIMESTAMP: старая правка не затирает новую"
    );
    assert!(shown.last().is_some_and(|message| message.edited));
    assert_eq!(
        chats
            .edit_at(bob, &sent.id, "чужая", now + 3_000)
            .await
            .expect("правка"),
        Edited::NotAuthor
    );
    assert_eq!(
        chats
            .edit_at(alice, "нет-такого", "x", now)
            .await
            .expect("правка"),
        Edited::Missing
    );
    let members: Option<(Vec<i64>,)> = session
        .query("SELECT members FROM chats WHERE chat = ?", (chat.as_str(),))
        .await
        .expect("участники")
        .maybe_first_row_typed()
        .expect("строка");
    assert_eq!(
        members.map(|(members,)| members.len()),
        Some(2),
        "участники в set"
    );
    chats.mark_read(bob, alice).await.expect("прочитано");
    for n in 0..(UNREAD_CAP + 5) {
        chats
            .append_at(&letter(1, 2, &format!("ещё {n}")), now_micros())
            .await
            .expect("отправка");
    }
    assert_eq!(
        chats.unread(bob, alice).await.expect("счётчик"),
        UNREAD_CAP,
        "предел «99+»"
    );
    assert_eq!(
        chats.unread(alice, bob).await.expect("счётчик"),
        0,
        "свои не считаются"
    );
    chats.mark_read(bob, alice).await.expect("прочитано");
    assert_eq!(chats.unread(bob, alice).await.expect("счётчик"), 0);
    let list = chats.chats(bob, 10).await.expect("чаты");
    assert_eq!(list[0].unread, 0);
    let fleeting = chats
        .send_for(letter(2, 1, "исчезну"), 1)
        .await
        .expect("с TTL");
    let seen = chats.history(alice, bob, 5).await;
    assert!(seen.iter().any(|message| message.id == fleeting.id));
    tokio::time::sleep(Duration::from_millis(2_500)).await;
    let later = chats.history(alice, bob, 5).await;
    assert!(
        !later.iter().any(|message| message.id == fleeting.id),
        "TTL убрал сообщение"
    );
    drop_keyspace(&config).await.expect("keyspace убран");
}
