use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use api_gateway::Config;
use contracts::{
    Changed, CommentId, CommentView, EventBody, MarkedView, MessageView, NoticeKind, NoticeView,
    NoticesView, PostId, PostView, Push, UnreadView, UserId, Value, decode,
};
use nats_engine::{Bus, BusConfig, Meta, Pattern, Subject};
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader, duplex};
use tokio::time::sleep;

use crate::support::{DEADLINE, Gateway, Peer, nats_url, ok, within};

fn target(user: UserId) -> String {
    format!(r#"{{"target": "{user}"}}"#)
}

fn post_ref(post: PostId) -> String {
    format!(r#"{{"post": "{post}"}}"#)
}

async fn bus() -> Bus {
    Bus::connect(BusConfig::new(nats_url()))
        .await
        .expect("NATS курса запущен?")
}

#[tokio::test]
async fn a_follow_rings_the_bell_of_the_target() {
    let stack = Gateway::start(Config::default()).await;
    let (alice, mut phone) = stack.member("alice").await;
    let (bob, mut other) = stack.member("bob").await;
    let _ = ok!(other, "users.follow", target(alice.id) => Changed);
    assert_eq!(
        phone.push().await,
        Push::Followed {
            to: alice.id,
            by: bob.id
        },
        "push пришёл событием: user-service, NATS, notify-service, шлюз"
    );
    let unread = ok!(phone, "notify.unread", "{}" => UnreadView);
    assert_eq!(unread.unread, 1, "колокольчик знает о подписке");
    let list = ok!(phone, "notify.list", r#"{"limit": 5}"# => NoticesView);
    assert_eq!(
        list.notices
            .iter()
            .map(|notice| (notice.to, notice.kind, notice.actor, notice.read))
            .collect::<Vec<_>>(),
        [(alice.id, NoticeKind::Followed, bob.id, false)]
    );
    let marked = ok!(phone, "notify.read", "{}" => MarkedView);
    assert_eq!(marked.marked, 1);
    let after = ok!(phone, "notify.unread", "{}" => UnreadView);
    assert_eq!(after.unread, 0, "прочитано");
    let theirs = ok!(other, "notify.unread", "{}" => UnreadView);
    assert_eq!(
        theirs.unread, 0,
        "колокольчик у каждого свой: пользователь берётся из токена, а не из data"
    );
    stack.stop().await;
}

#[tokio::test]
async fn likes_and_comments_reach_the_author_and_own_actions_stay_quiet() {
    let stack = Gateway::start(Config::default()).await;
    let (alice, mut author) = stack.member("alice").await;
    let (bob, mut fan) = stack.member("bob").await;
    let post = ok!(author, "posts.create", r#"{"text": "обсудим"}"# => PostView);
    let _ = ok!(fan, "posts.like", post_ref(post.id) => Changed);
    let said = ok!(
        fan,
        "posts.comment",
        format!(r#"{{"post": "{}", "text": "согласен"}}"#, post.id) => CommentView
    );
    assert_eq!(
        said,
        CommentView {
            id: CommentId(1),
            post: post.id
        },
        "комментарий ушёл в post-service, ответ называет его id"
    );
    let mut first = vec![author.push().await, author.push().await];
    first.sort_by_key(|push| format!("{push:?}"));
    assert_eq!(
        first,
        [
            Push::Commented {
                to: alice.id,
                by: bob.id,
                post: post.id
            },
            Push::Liked {
                to: alice.id,
                by: bob.id,
                post: post.id
            },
        ],
        "лайк и комментарий доходят до автора"
    );
    let _ = ok!(author, "posts.like", post_ref(post.id) => Changed);
    let _ = ok!(
        author,
        "posts.comment",
        format!(r#"{{"post": "{}", "text": "сам себе"}}"#, post.id) => CommentView
    );
    let _ = ok!(fan, "users.follow", target(alice.id) => Changed);
    assert_eq!(
        author.push().await,
        Push::Followed {
            to: alice.id,
            by: bob.id
        },
        "свой лайк и свой комментарий не звонят: следующим пришла подписка bob"
    );
    let bell = ok!(author, "notify.unread", "{}" => UnreadView);
    assert_eq!(bell.unread, 3);
    stack.stop().await;
}

#[tokio::test]
async fn a_new_post_reaches_followers_through_notify_service() {
    let stack = Gateway::start(Config::default()).await;
    let (alice, mut writer) = stack.member("alice").await;
    let (bob, mut reader) = stack.member("bob").await;
    let (carol, mut stranger) = stack.member("carol").await;
    let _ = ok!(reader, "users.follow", target(alice.id) => Changed);
    assert_eq!(
        writer.push().await,
        Push::Followed {
            to: alice.id,
            by: bob.id
        }
    );
    let post = ok!(writer, "posts.create", r#"{"text": "всем привет"}"# => PostView);
    assert_eq!(
        reader.push().await,
        Push::NewPost {
            to: bob.id,
            author: alice.id,
            post: post.id
        },
        "подписчик узнаёт о посте: веер по audience делает notify-service"
    );
    let _ = ok!(writer, "users.follow", target(carol.id) => Changed);
    assert_eq!(
        stranger.push().await,
        Push::Followed {
            to: carol.id,
            by: alice.id
        },
        "carol не подписана на alice: о посте ей не пришло, первым пришла подписка"
    );
    stack.stop().await;
}

#[tokio::test]
async fn without_notify_service_the_gateway_pushes_nothing_by_itself() {
    let mut stack = Gateway::start(Config::default()).await;
    let (alice, mut phone) = stack.member("alice").await;
    let (bob, mut other) = stack.member("bob").await;
    stack.halt("notify-service").await;
    let _ = ok!(other, "users.follow", target(alice.id) => Changed);
    let _ = ok!(
        other,
        "chat.send",
        format!(r#"{{"peer": "{}", "text": "ты тут?"}}"#, alice.id) => MessageView
    );
    assert_eq!(
        phone.push().await,
        Push::Message {
            to: alice.id,
            from: bob.id
        },
        "самодельных пушей больше нет: о подписке без notify-service никто не сказал, \
         а сообщение пришло потоком chat-service"
    );
    let bell = phone.fails("notify.unread", "{}").await;
    assert_eq!(
        bell.code, "unavailable",
        "некому ответить на запрос: шлюз говорит unavailable, а не висит: {bell:?}"
    );
    stack.stop().await;
}

#[tokio::test]
async fn a_notice_from_the_bus_reaches_only_its_addressee() {
    let stack = Gateway::start(Config::default()).await;
    let (alice, mut phone) = stack.member("alice").await;
    let (bob, mut other) = stack.member("bob").await;
    let bus = bus().await;
    let notice = |id: u64, to: UserId, actor: UserId| NoticeView {
        id,
        to,
        kind: NoticeKind::Liked,
        actor,
        post: Some(PostId(7)),
        read: false,
    };
    let send = |to: UserId, notice: NoticeView| {
        let words = stack.topics.notice(to);
        let words: Vec<&str> = words.iter().map(String::as_str).collect();
        let subject = Subject::new(&words).expect("тема");
        let bus = bus.clone();
        async move {
            bus.publish(&subject, &notice, &Meta::fresh())
                .await
                .expect("уведомление ушло");
        }
    };
    send(alice.id, notice(100, bob.id, alice.id)).await;
    send(alice.id, notice(101, alice.id, bob.id)).await;
    assert_eq!(
        phone.push().await,
        Push::Liked {
            to: alice.id,
            by: bob.id,
            post: PostId(7)
        },
        "уведомление из темы alice доходит до alice"
    );
    send(bob.id, notice(102, bob.id, alice.id)).await;
    assert_eq!(
        other.push().await,
        Push::Liked {
            to: bob.id,
            by: alice.id,
            post: PostId(7)
        },
        "уведомление для bob в теме alice выброшено: тема и адресат должны совпасть"
    );
    stack.stop().await;
}

#[tokio::test]
async fn the_request_id_rides_the_event() {
    let stack = Gateway::start(Config::default()).await;
    let (alice, _) = stack.member("alice").await;
    let (_, mut other) = stack.member("bob").await;
    let bus = bus().await;
    let watch = Pattern::parse(&stack.topics.all_events().join(".")).expect("шаблон");
    let mut events = bus
        .subscribe::<EventBody>(&watch)
        .await
        .expect("подписка на события");
    let raw = other.call("users.follow", &target(alice.id)).await;
    let reply: Value = decode(&raw).expect("ответ JSON");
    let request_id = reply
        .get("request_id")
        .and_then(Value::as_str)
        .expect("в ответе шлюза есть request_id")
        .to_string();
    let seen = within("событие подписки в шине", async {
        loop {
            if let Some(Ok(envelope)) = events.next_within(DEADLINE).await {
                if envelope.subject.as_str().contains(".user.followed.") {
                    return envelope;
                }
            }
        }
    })
    .await;
    assert_eq!(
        seen.meta.map(|meta| meta.request_id),
        Some(request_id),
        "номер WS-запроса прошёл шлюз, gRPC и стал заголовком события"
    );
    stack.stop().await;
}

#[tokio::test]
async fn the_bell_checks_its_data_and_its_caller() {
    let stack = Gateway::start(Config::default()).await;
    let (_, mut phone) = stack.member("alice").await;
    for limit in [0, 101] {
        let refused = phone
            .fails("notify.list", &format!(r#"{{"limit": {limit}}}"#))
            .await;
        assert_eq!(refused.code, "bad_data", "limit {limit}: {refused:?}");
    }
    let empty = phone
        .fails("posts.comment", r#"{"post": "p_1", "text": "   "}"#)
        .await;
    assert_eq!(
        empty.code, "validation",
        "пустой комментарий отвергает post-service: {empty:?}"
    );
    let mut guest = Peer::connect(stack.http, "/ws").await;
    let anonymous = guest.fails("notify.unread", "{}").await;
    assert_eq!(anonymous.code, "unauthorized", "{anonymous:?}");
    stack.stop().await;
}

#[tokio::test]
async fn the_course_client_rings_and_reads_the_bell() {
    let stack = Gateway::start(Config::default()).await;
    let (_, mut fan) = stack.member("bob").await;
    let (alice, _) = stack.member("alice").await;
    let (mut keys, typed) = duplex(4096);
    let (shown, mut glass) = duplex(64 * 1024);
    let (http, dialer) = (stack.http.to_string(), stack.dialer.clone());
    let session = tokio::spawn(async move {
        let _ = social_cli::run_with(&http, dialer, BufReader::new(typed), shown).await;
    });
    let screen = Arc::new(Mutex::new(String::new()));
    let sink = Arc::clone(&screen);
    let reader = tokio::spawn(async move {
        let mut chunk = [0u8; 4096];
        while let Ok(n) = glass.read(&mut chunk).await {
            if n == 0 {
                break;
            }
            sink.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push_str(&String::from_utf8_lossy(&chunk[..n]));
        }
    });
    let wait = |needle: String| {
        let screen = Arc::clone(&screen);
        async move {
            let began = Instant::now();
            loop {
                let now = screen
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .clone();
                if now.contains(&needle) {
                    return;
                }
                assert!(began.elapsed() < DEADLINE, "нет «{needle}» в\n{now}");
                sleep(Duration::from_millis(20)).await;
            }
        }
    };
    keys.write_all("login alice correct-horse\npost звоночек\n".as_bytes())
        .await
        .expect("ввод");
    wait(String::from("@alice [1] звоночек")).await;
    let post = PostId(1);
    let _ = ok!(fan, "posts.like", post_ref(post) => Changed);
    let _ = ok!(
        fan,
        "posts.comment",
        format!(r#"{{"post": "{post}", "text": "класс"}}"#) => CommentView
    );
    let _ = ok!(fan, "users.follow", target(alice.id) => Changed);
    for line in [
        "* @bob лайкнул твой пост [1]",
        "* @bob прокомментировал твой пост [1]",
        "* на тебя подписался @bob",
    ] {
        wait(line.to_string()).await;
    }
    keys.write_all("bell\n".as_bytes()).await.expect("ввод");
    wait(String::from("колокольчик: непрочитанных 3")).await;
    wait(String::from("  • @bob прокомментировал твой пост [1]")).await;
    keys.write_all("read\ncomment 1 и тебе спасибо\nbell\nquit\n".as_bytes())
        .await
        .expect("ввод");
    wait(String::from("прочитано уведомлений: 3")).await;
    wait(String::from("комментарий c_2 к посту [1]")).await;
    wait(String::from("колокольчик: непрочитанных 0")).await;
    wait(String::from("    @bob подписался на тебя")).await;
    let _ = within("клиент закрылся", session).await;
    reader.abort();
    stack.stop().await;
}
