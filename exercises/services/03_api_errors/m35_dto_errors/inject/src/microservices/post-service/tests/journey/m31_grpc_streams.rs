use std::time::Duration;

use grpc_kit::Stamp;
use post_service::{Config, run};
use proto::social::v1::feed_event::Kind;
use proto::social::v1::posts_client::PostsClient;
use proto::social::v1::users_client::UsersClient;
use proto::social::v1::{
    CreatePostRequest, CreateProfileRequest, FeedEvent, PairRequest, WatchRequest,
};
use service_runtime::CancellationToken;
use tokio::net::TcpListener;
use tokio::time::timeout;
use tonic::Code;
use tonic::Streaming;
use tonic::service::interceptor::InterceptedService;
use tonic::transport::Channel;

const WAIT: Duration = Duration::from_secs(5);

async fn listener() -> (TcpListener, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("порт");
    let addr = listener.local_addr().expect("адрес").to_string();
    (listener, addr)
}

fn lazy(addr: &str) -> Channel {
    Channel::from_shared(format!("http://{addr}"))
        .expect("адрес")
        .connect_lazy()
}

fn posts(channel: &Channel, user: u64) -> PostsClient<InterceptedService<Channel, Stamp>> {
    PostsClient::with_interceptor(channel.clone(), Stamp::fresh().user(user))
}

async fn next(stream: &mut Streaming<FeedEvent>) -> Kind {
    timeout(WAIT, stream.message())
        .await
        .expect("событие ленты не пришло за 5 с")
        .expect("поток жив")
        .expect("поток не кончился")
        .kind
        .expect("в событии есть kind")
}

fn text(kind: Kind) -> (String, bool) {
    match kind {
        Kind::History(post) => (post.text, false),
        Kind::Live(post) => (post.text, true),
        Kind::CaughtUp(..) => panic!("ждали пост, а пришла отметка caught_up"),
    }
}

#[tokio::test]
async fn watch_streams_history_then_live_posts_of_follows() {
    let stop = CancellationToken::new();
    let (users_listener, users_addr) = listener().await;
    let (posts_listener, posts_addr) = listener().await;
    let users_task = tokio::spawn(user_service::run(
        users_listener,
        user_service::Config::default(),
        stop.clone(),
    ));
    let config = Config {
        users: users_addr.clone(),
        ..Config::default()
    };
    let posts_task = tokio::spawn(run(posts_listener, config, stop.clone()));
    let (users, channel) = (lazy(&users_addr), lazy(&posts_addr));
    for name in ["alice", "bob", "carol"] {
        let ask = CreateProfileRequest {
            username: name.to_string(),
        };
        UsersClient::new(users.clone())
            .create_profile(ask)
            .await
            .expect("профиль");
    }
    let (alice, bob, carol) = (1, 2, 3);
    UsersClient::with_interceptor(users.clone(), Stamp::fresh().user(bob))
        .follow(PairRequest { target_id: alice })
        .await
        .expect("bob подписан на alice");
    let say = |author: u64, text: &'static str| {
        let mut client = posts(&channel, author);
        async move {
            let ask = CreatePostRequest {
                text: text.to_string(),
                ..Default::default()
            };
            client.create_post(ask).await.expect("пост");
        }
    };
    say(alice, "раз").await;
    say(alice, "два").await;
    say(carol, "чужой").await;
    let anonymous = PostsClient::new(channel.clone())
        .watch(WatchRequest { recent: 5 })
        .await
        .expect_err("без x-user-id");
    assert_eq!(anonymous.code(), Code::Unauthenticated);
    let mut stream = posts(&channel, bob)
        .watch(WatchRequest { recent: 5 })
        .await
        .expect("поток ленты открывается")
        .into_inner();
    assert_eq!(text(next(&mut stream).await), ("два".to_string(), false));
    assert_eq!(text(next(&mut stream).await), ("раз".to_string(), false));
    assert!(
        matches!(next(&mut stream).await, Kind::CaughtUp(..)),
        "после истории отметка: дальше только новое"
    );
    say(carol, "чужой новый").await;
    say(alice, "три").await;
    assert_eq!(
        text(next(&mut stream).await),
        ("три".to_string(), true),
        "новый пост подписки идёт потоком, чужой нет"
    );
    stop.cancel();
    let ended = timeout(WAIT, stream.message())
        .await
        .expect("поток закрывается на остановке");
    assert!(
        !matches!(ended, Ok(Some(..))),
        "post-service сам закрывает живые потоки, иначе мягкая остановка ждала бы их вечно: {ended:?}"
    );
    let posts = timeout(WAIT, posts_task)
        .await
        .expect("post-service остановился");
    assert!(
        posts.expect("задача").is_ok(),
        "post-service останавливается без ошибки"
    );
    let users = timeout(WAIT, users_task)
        .await
        .expect("user-service остановился");
    assert!(
        users.expect("задача").is_ok(),
        "user-service останавливается без ошибки"
    );
}
