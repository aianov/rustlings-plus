use std::time::Duration;

use api_gateway::Config;
use contracts::{Changed, PostView, Push, UserView, decode};
use grpc_kit::{Stamp, Tls, channel};
use proto::social::v1::CreatePostRequest;
use proto::social::v1::posts_client::PostsClient;
use social_cli::{Dialer, Web, run_with};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tonic::Code;
use tonic_health::pb::HealthCheckRequest;
use tonic_health::pb::health_client::HealthClient;

use crate::support::{Gateway, issue_certs, ok, within};

const WAIT: Duration = Duration::from_secs(5);

fn certs_of(gateway: &Gateway) -> std::path::PathBuf {
    gateway.certs.clone().expect("стек поднят с сертификатами")
}

#[tokio::test]
async fn https_and_wss_with_our_own_ca() {
    let stack = Gateway::secure("m32-https", Config::default()).await;
    let (alice, mut writer) = stack.member("alice").await;
    let (bob, mut reader) = stack.member("bob").await;
    let _ = ok!(reader, "users.follow", format!(r#"{{"target": "{}"}}"#, alice.id) => Changed);
    assert_eq!(
        writer.push().await,
        Push::Followed {
            to: alice.id,
            by: bob.id
        },
        "пуши ходят по wss:// так же, как по ws://"
    );
    let post = ok!(writer, "posts.create", r#"{"text": "по шифрованному каналу"}"# => PostView);
    assert_eq!(post.author, alice.id);
    let profile = within(
        "профиль по HTTPS",
        stack.web().request("GET", "/users/alice", None, None),
    )
    .await
    .expect("HTTPS шлюза отвечает");
    assert_eq!(profile.status, 200, "{profile:?}");
    let user: UserView = decode(&profile.body).expect("профиль разбирается");
    assert_eq!(user.id, alice.id);
    let health = within(
        "здоровье по HTTPS",
        stack.web().request("GET", "/health", None, None),
    )
    .await
    .expect("HTTPS шлюза отвечает");
    assert_eq!(
        health.status, 200,
        "шлюз видит SERVING всех сервисов через mTLS"
    );
    stack.stop().await;
}

#[tokio::test]
async fn plaintext_and_strangers_are_turned_away() {
    let stack = Gateway::secure("m32-strangers", Config::default()).await;
    let mut plain = TcpStream::connect(stack.http)
        .await
        .expect("порт шлюза открыт");
    plain
        .write_all(b"GET /health HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
        .await
        .expect("пишется");
    let mut answer = Vec::new();
    let _ = timeout(WAIT, plain.read_to_end(&mut answer)).await;
    assert!(
        !String::from_utf8_lossy(&answer).starts_with("HTTP/1.1 200"),
        "открытым текстом шлюз больше не отвечает"
    );
    let other = issue_certs("m32-other-ca");
    let foreign = Dialer::trusting(&std::fs::read(other.join("ca.pem")).expect("чужой CA"))
        .expect("клиент с чужим CA");
    let refused = Web::with(&stack.http.to_string(), foreign)
        .request("GET", "/health", None, None)
        .await;
    assert!(
        refused.is_err(),
        "клиент, который доверяет другому CA, не верит сертификату шлюза: {refused:?}"
    );
    let ask = || HealthCheckRequest {
        service: String::new(),
    };
    let services = stack.services.clone();
    for (name, addr) in [
        ("auth-service", services.auth),
        ("user-service", services.users),
        ("post-service", services.posts),
        ("chat-service", services.chat),
    ] {
        let open = channel(&addr, name, None).expect("канал");
        assert!(
            HealthClient::new(open).check(ask()).await.is_err(),
            "{name} не говорит открытым текстом"
        );
        let stranger = Tls::load(&other, "api-gateway").expect("чужие сертификаты");
        let forged = channel(&addr, name, Some(&stranger)).expect("канал");
        assert!(
            HealthClient::new(forged).check(ask()).await.is_err(),
            "{name} не пускает сертификат от чужого CA, даже с именем api-gateway"
        );
    }
    stack.stop().await;
}

#[tokio::test]
async fn post_service_hears_only_the_gateway() {
    let stack = Gateway::secure("m32-gate", Config::default()).await;
    let alice = stack.register("alice").await;
    let certs = certs_of(&stack);
    let post = |who: &str| {
        let tls = Tls::load(&certs, who).expect("сертификаты");
        let channel = channel(&stack.services.posts, "post-service", Some(&tls)).expect("канал");
        let mut client = PostsClient::with_interceptor(channel, Stamp::fresh().user(alice.id.0));
        async move {
            let ask = CreatePostRequest {
                text: format!("от имени {}", tls.name),
                ..Default::default()
            };
            client.create_post(ask).await
        }
    };
    for intruder in ["user-service", "chat-service", "notify-service"] {
        let refused = post(intruder).await.expect_err("не шлюз");
        assert_eq!(
            refused.code(),
            Code::PermissionDenied,
            "{intruder}: сертификат нашего CA, но в SAN не api-gateway и не auth-service: {refused:?}"
        );
    }
    let saga = post("auth-service")
        .await
        .expect("с m42 auth-service ведёт сагу удаления аккаунта и тоже ходит в post-service");
    assert_eq!(saga.into_inner().author_id, alice.id.0);
    let accepted = post("api-gateway")
        .await
        .expect("шлюзу post-service отвечает");
    assert_eq!(accepted.into_inner().author_id, alice.id.0);
    stack.stop().await;
}

#[tokio::test]
async fn course_client_goes_over_https_and_wss() {
    let stack = Gateway::secure("m32-client", Config::default()).await;
    let mut screen = Vec::new();
    within(
        "сеанс клиента по TLS",
        run_with(
            &stack.http.to_string(),
            stack.dialer.clone(),
            "register alice correct-horse\nlogin alice correct-horse\npost зашифровано\nquit\n"
                .as_bytes(),
            &mut screen,
        ),
    )
    .await
    .expect("сеанс проходит");
    let mut alice = stack.device("alice").await;
    let _ = stack
        .feed_until(&mut alice, "пост alice в её ленте", |page| {
            !page.posts.is_empty()
        })
        .await;
    within(
        "второй сеанс клиента по TLS",
        run_with(
            &stack.http.to_string(),
            stack.dialer.clone(),
            "login alice correct-horse\nfeed\nquit\n".as_bytes(),
            &mut screen,
        ),
    )
    .await
    .expect("второй сеанс проходит");
    let screen = String::from_utf8(screen).expect("клиент пишет UTF-8");
    assert!(
        screen.contains(&format!("шлюз https://{}", stack.http))
            && screen.contains("привет, @alice")
            && screen.matches("@alice [1] зашифровано").count() == 2,
        "клиент курса ходит по https:// и wss://, доверяя только CA из certs/:\n{screen}"
    );
    stack.stop().await;
}
