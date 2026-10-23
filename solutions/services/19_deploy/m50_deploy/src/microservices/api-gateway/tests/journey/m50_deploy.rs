use std::time::Duration;

use api_gateway::{Config, Routes, Services, run_with};
use contracts::PostView;
use service_runtime::CancellationToken;
use social_cli::Web;

use crate::support::{Gateway, Peer, listener, nats, ok, within};

#[tokio::test]
async fn posts_go_round_robin_past_a_dead_replica() {
    let stack = Gateway::start(Config::default()).await;
    stack.register("alice").await;
    let token = stack.login("alice").await.token;
    let (http, addr) = listener().await;
    let services = Services {
        posts: format!("127.0.0.1:1,{}", stack.services.posts),
        ..stack.services.clone()
    };
    let config = Config {
        services,
        nats: Some(nats("api-gateway-replicas")),
        topics: stack.topics.clone(),
        ..Config::default()
    };
    let stop = CancellationToken::new();
    let task = tokio::spawn(run_with(http, config, Routes::social(), stop.clone()));
    within(
        "второй шлюз готов, хотя одна реплика мертва",
        async {
            loop {
                let reply = Web::new(&addr.to_string())
                    .request("GET", "/health", None, None)
                    .await;
                if reply.is_ok_and(|reply| reply.status == 200) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        },
    )
    .await;
    let mut peer = Peer::connect(addr, &format!("/ws?token={token}")).await;
    for n in 0..6 {
        let post = ok!(peer, "posts.create", format!(r#"{{"text": "пост {n}"}}"#) => PostView);
        assert_eq!(
            post.text,
            format!("пост {n}"),
            "каждый запрос попал в живую реплику"
        );
    }
    stop.cancel();
    let _ = within("остановка второго шлюза", task).await;
    stack.stop().await;
}
