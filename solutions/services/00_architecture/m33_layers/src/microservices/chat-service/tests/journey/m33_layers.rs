use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use chat_service::repository::BoxFuture;
use chat_service::service::DirectoryError;
use chat_service::{ChatError, ChatService, Hub, MemoryChats, UserDirectory};
use contracts::UserId;

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

#[tokio::test]
async fn rules_and_the_peer_check_live_in_the_service() {
    let hub = Arc::new(Hub::default());
    let chat = ChatService::new(Arc::new(MemoryChats::new()), Arc::clone(&hub));
    let users = Directory::default();
    let (alice, bob) = (UserId(1), UserId(2));
    for (to, text, expected) in [(alice, "я", "rejected"), (bob, "   ", "rejected")] {
        let refused = chat
            .send(&users, alice, to, text)
            .await
            .expect_err("плохое сообщение");
        assert_eq!(refused.code(), expected, "{refused}");
    }
    assert_eq!(
        users.asked.load(Ordering::SeqCst),
        0,
        "себе и пустое отсекаются до похода в user-service"
    );
    let missing = chat
        .send(&users, alice, UserId(9), "ау")
        .await
        .expect_err("нет собеседника");
    assert!(matches!(missing, ChatError::UnknownPeer(UserId(9))));
    let mut inbox = chat.subscribe(bob).expect("подписка");
    let sent = chat
        .send(&users, alice, bob, "привет")
        .await
        .expect("сообщение");
    assert_eq!(
        inbox.recv().await,
        Some(sent),
        "хаб раздаёт сообщение подписчикам получателя"
    );
    let history = chat.history(&users, bob, alice, 10).await.expect("история");
    assert_eq!(history.len(), 1);
    hub.close();
    assert!(
        matches!(chat.subscribe(bob), Err(ChatError::Closing)),
        "после close новые подписки не открываются"
    );
}
