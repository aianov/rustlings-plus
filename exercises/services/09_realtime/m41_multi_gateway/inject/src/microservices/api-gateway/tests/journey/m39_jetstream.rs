use api_gateway::Config;
use contracts::{Changed, MessageView, PostView, Push, UnreadView};
use service_runtime::CancellationToken;

use crate::support::{Gateway, letter, nats, ok, plain};

#[tokio::test]
async fn notifications_survive_a_notify_service_restart() {
    let mut stack = Gateway::start(Config::default()).await;
    let (alice, mut phone) = stack.member("alice").await;
    let (bob, mut other) = stack.member("bob").await;
    let post = ok!(phone, "posts.create", r#"{"text": "пока никого"}"# => PostView);
    stack.halt("notify-service").await;
    let _ = ok!(other, "users.follow", format!(r#"{{"target": "{}"}}"#, alice.id) => Changed);
    let _ = ok!(other, "posts.like", format!(r#"{{"post": "{}"}}"#, post.id) => Changed);
    let _ = ok!(
        other,
        "chat.send",
        format!(r#"{{"peer": "{}", "text": "ты тут?"}}"#, alice.id) => MessageView
    );
    assert_eq!(
        plain(phone.push().await),
        letter(alice.id, bob.id, "ты тут?"),
        "пока notify-service лежит, уведомлений нет: первым пришло сообщение чата"
    );
    let config = notify_service::Config {
        users: stack.services.users.clone(),
        nats: nats("notify-service"),
        topics: stack.topics.clone(),
        ..notify_service::Config::default()
    };
    let stop = CancellationToken::new();
    let revived = notify_service::start(None, config, stop.clone())
        .await
        .expect("notify-service снова поднялся");
    let mut caught_up = vec![phone.push().await, phone.push().await];
    caught_up.sort_by_key(|push| format!("{push:?}"));
    assert_eq!(
        caught_up,
        [
            Push::Followed {
                to: alice.id,
                by: bob.id
            },
            Push::Liked {
                to: alice.id,
                by: bob.id,
                post: post.id
            },
        ],
        "события лежали в stream: после перезапуска notify-service догнал их сам"
    );
    let bell = ok!(phone, "notify.unread", "{}" => UnreadView);
    assert_eq!(
        bell.unread, 2,
        "колокольчик нового экземпляра знает оба уведомления"
    );
    stop.cancel();
    revived.finished().await.expect("остановился чисто");
    stack.stop().await;
}
