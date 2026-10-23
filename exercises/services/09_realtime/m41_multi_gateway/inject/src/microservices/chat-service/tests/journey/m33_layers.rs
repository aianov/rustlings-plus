use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use chat_service::repository::BoxFuture;
use chat_service::service::DirectoryError;
use chat_service::{
    ChatError, ChatService, MemoryChats, MessageText, Offline, OutboxRepository, UserDirectory,
};
use contracts::{Fact, Origin, UserId};

#[derive(Default)]
struct Directory {
    asked: AtomicUsize,
}

impl UserDirectory for Directory {
    fn exists(&self, id: UserId) -> BoxFuture<'_, Result<bool, DirectoryError>> {
        self.asked.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move { Ok(id.0 <= 2) })
    }
}

fn text(raw: &str) -> MessageText {
    MessageText::try_from(raw).expect("текст сообщения")
}

#[tokio::test]
async fn rules_and_the_peer_check_live_in_the_service() {
    let chats = Arc::new(MemoryChats::new());
    let chat = ChatService::new(chats.clone(), Arc::new(Offline));
    let users = Directory::default();
    let origin = Origin::new("m33-chat");
    let (alice, bob) = (UserId(1), UserId(2));
    let refused = chat
        .send(&users, alice, alice, text("я"), &origin)
        .await
        .expect_err("себе");
    assert_eq!(refused.code(), "rejected", "{refused}");
    assert_eq!(
        MessageText::try_from("   ").map_err(|violation| violation.code),
        Err("blank"),
        "пустое сообщение даже не собирается в MessageText: service его не увидит"
    );
    assert_eq!(
        users.asked.load(Ordering::SeqCst),
        0,
        "себе отсекается до похода в user-service"
    );
    let missing = chat
        .send(&users, alice, UserId(9), text("ау"), &origin)
        .await
        .expect_err("нет собеседника");
    assert!(matches!(missing, ChatError::UnknownPeer(UserId(9))));
    assert_eq!(
        chats.waiting().await,
        0,
        "отказы не оставляют событий в outbox"
    );
    chat.send(&users, alice, bob, text("привет"), &origin)
        .await
        .expect("сообщение");
    let rows = chats.unpublished(10).await;
    assert!(
        matches!(rows.as_slice(), [row] if matches!(row.event.fact(), Ok(Some(Fact::MessageSent(_))))),
        "с m41 сервис не раздаёт сообщения сам: событие chat.sent ложится в outbox вместе с сообщением: {rows:?}"
    );
    let history = chat.history(&users, bob, alice, 10).await.expect("история");
    assert_eq!(history.len(), 1);
    let typing = chat
        .typing(&users, alice, bob, &origin)
        .await
        .expect_err("без брокера «печатает» не доставить");
    assert_eq!(typing.code(), "unavailable", "{typing}");
}
