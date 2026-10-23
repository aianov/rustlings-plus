use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use api_gateway::{Config, Routes, Services, run_with};
use contracts::{Changed, FeedPage, MessageView, PostView, Push, UserView};
use grpc_kit::Stamp;
use http_engine::{Limits, ServerConfig};
use proto::social::v1::auth_client::AuthClient;
use proto::social::v1::users_client::UsersClient;
use proto::social::v1::{
    AvatarRequest, GetAccountRequest, GetProfileRequest, PutAvatarRequest, ValidateTokenRequest,
};
use service_runtime::CancellationToken;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::task::JoinHandle;
use tonic::Code;
use tonic::transport::Channel;

use crate::fakes::{LEAK, fake_auth, fake_users};
use crate::support::{Gateway, Journals, Peer, letter, listener, ok, plain, within};

struct Extra {
    addr: SocketAddr,
    stop: CancellationToken,
    task: JoinHandle<api_gateway::Result<()>>,
}

impl Extra {
    async fn start(services: Services) -> Extra {
        let (http, addr) = listener().await;
        let stop = CancellationToken::new();
        let config = Config {
            services,
            ..Config::default()
        };
        let task = tokio::spawn(run_with(http, config, Routes::social(), stop.clone()));
        Extra { addr, stop, task }
    }

    async fn peer(&self, token: &str) -> Peer {
        Peer::connect(self.addr, &format!("/ws?token={token}")).await
    }

    async fn stop(self) {
        self.stop.cancel();
        let finished = within("остановка шлюза", self.task)
            .await
            .expect("задача шлюза не падает");
        assert!(
            finished.is_ok(),
            "шлюз останавливается без ошибки: {finished:?}"
        );
    }
}

async fn counting_proxy(upstream: &str) -> (String, Arc<AtomicUsize>) {
    let upstream: SocketAddr = upstream.parse().expect("адрес сервиса это ip:порт");
    let (listener, addr) = listener().await;
    let accepted = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&accepted);
    tokio::spawn(async move {
        while let Ok((mut inbound, _)) = listener.accept().await {
            seen.fetch_add(1, Ordering::SeqCst);
            tokio::spawn(async move {
                if let Ok(mut outbound) = TcpStream::connect(upstream).await {
                    let _ = tokio::io::copy_bidirectional(&mut inbound, &mut outbound).await;
                }
            });
        }
    });
    (addr.to_string(), accepted)
}

async fn dead_addr() -> String {
    let (listener, addr) = listener().await;
    drop(listener);
    addr.to_string()
}

async fn raw(
    addr: SocketAddr,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> (u16, Vec<u8>) {
    let mut stream = within("подключение к HTTP", TcpStream::connect(addr))
        .await
        .expect("HTTP-порт принимает соединения");
    let mut head = format!("{method} {path} HTTP/1.1\r\nHost: test\r\nConnection: close\r\n");
    for (name, value) in headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str(&format!("Content-Length: {}\r\n\r\n", body.len()));
    let mut bytes = head.into_bytes();
    bytes.extend_from_slice(body);
    stream.write_all(&bytes).await.expect("запрос уходит");
    let mut answer = Vec::new();
    within("ответ HTTP", stream.read_to_end(&mut answer))
        .await
        .expect("ответ читается");
    let end = answer
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("в ответе есть конец заголовков");
    let status = String::from_utf8_lossy(&answer[..end])
        .split(' ')
        .nth(1)
        .and_then(|code| code.parse().ok())
        .expect("строка статуса разбирается");
    (status, answer[end + 4..].to_vec())
}

#[tokio::test]
async fn auth_lives_in_its_own_service() {
    let stack = Gateway::start(Config::default()).await;
    for path in ["/auth/register", "/auth/login"] {
        let relayed = stack
            .http("POST", path, Some(r#"{"username": "alice"}"#))
            .await;
        assert_eq!(
            relayed.status, 422,
            "{path} шлюз не решает сам, а передаёт в auth-service, и тот проверяет поля: {relayed:?}"
        );
    }
    let alice = stack.register("alice").await;
    let login = stack.login("alice").await;
    let mut auth = AuthClient::connect(format!("http://{}", stack.services.auth))
        .await
        .expect("gRPC auth-service отвечает");
    let account = auth
        .validate_token(ValidateTokenRequest {
            token: login.token.clone(),
        })
        .await
        .expect("свежий токен проходит ValidateToken")
        .into_inner();
    assert_eq!(
        (account.user_id, account.username.as_str()),
        (alice.id.0, "alice"),
        "ValidateToken отвечает владельцем токена"
    );
    let forged = auth
        .validate_token(ValidateTokenRequest {
            token: "0123456789abcdef0123456789abcdef".to_string(),
        })
        .await
        .expect_err("чужой токен не проходит");
    assert_eq!(forged.code(), Code::Unauthenticated, "{forged:?}");
    let found = auth
        .get_account(GetAccountRequest {
            user_id: alice.id.0,
        })
        .await
        .expect("GetAccount находит alice")
        .into_inner();
    assert_eq!(found.username, "alice");
    let missing = auth
        .get_account(GetAccountRequest { user_id: 99 })
        .await
        .expect_err("учётной записи 99 нет");
    assert_eq!(missing.code(), Code::NotFound, "{missing:?}");
    let mut users = UsersClient::connect(format!("http://{}", stack.services.users))
        .await
        .expect("gRPC user-service отвечает");
    let profile = users
        .get_profile(GetProfileRequest {
            user_id: alice.id.0,
        })
        .await
        .expect("регистрация в auth-service создала профиль в user-service")
        .into_inner();
    assert_eq!(profile.username, "alice");
    let again = stack
        .auth_http(
            "POST",
            "/auth/register",
            Some(r#"{"username": "Alice", "password": "correct-horse"}"#),
        )
        .await;
    assert!(
        again.status == 409 && again.body.contains("conflict"),
        "занятый ник: user-service отвечает ALREADY_EXISTS, auth-service переводит в 409 conflict: {again:?}"
    );
    stack.stop().await;
}

#[tokio::test]
async fn gateway_keeps_one_channel_per_service() {
    let stack = Gateway::start(Config::default()).await;
    let (auth, auth_seen) = counting_proxy(&stack.services.auth).await;
    let (users, users_seen) = counting_proxy(&stack.services.users).await;
    let (posts, posts_seen) = counting_proxy(&stack.services.posts).await;
    let (chat, chat_seen) = counting_proxy(&stack.services.chat).await;
    let (feed, feed_seen) = counting_proxy(&stack.services.feed).await;
    let extra = Extra::start(Services {
        auth,
        auth_http: String::from("127.0.0.1:9"),
        users,
        posts,
        chat,
        feed,
    })
    .await;
    stack.register("alice").await;
    stack.register("bob").await;
    let mut peers = Vec::new();
    for _ in 0..3 {
        let login = stack.login("alice").await;
        peers.push(extra.peer(&login.token).await);
    }
    let work = |mut peer: Peer| async move {
        for _ in 0..4 {
            let bob = ok!(peer, "users.find", r#"{"username": "bob"}"# => UserView);
            assert_eq!(bob.username, "bob");
            let _ = ok!(peer, "posts.create", r#"{"text": "по одному каналу"}"# => PostView);
            let _ = ok!(peer, "posts.feed", "{}" => FeedPage);
        }
        let _ = ok!(peer, "users.follow", r#"{"target": "u_2"}"# => Changed);
        peer
    };
    let mut peers = peers.into_iter();
    let (Some(a), Some(b), Some(c)) = (peers.next(), peers.next(), peers.next()) else {
        panic!("три сокета alice открыты");
    };
    let _ = tokio::join!(work(a), work(b), work(c));
    let seen = [
        ("auth-service", auth_seen.load(Ordering::SeqCst)),
        ("user-service", users_seen.load(Ordering::SeqCst)),
        ("post-service", posts_seen.load(Ordering::SeqCst)),
        ("chat-service", chat_seen.load(Ordering::SeqCst)),
        ("feed-service", feed_seen.load(Ordering::SeqCst)),
    ];
    assert_eq!(
        seen,
        [
            ("auth-service", 1),
            ("user-service", 1),
            ("post-service", 1),
            ("chat-service", 1),
            ("feed-service", 1)
        ],
        "шлюз держит один Channel на сервис и мультиплексирует по нему все вызовы всех сокетов; \
         новое соединение на вызов это лишние рукопожатия и порты"
    );
    extra.stop().await;
    stack.stop().await;
}

#[tokio::test]
async fn request_id_follows_the_call_and_errors_do_not_leak() {
    let stop = CancellationToken::new();
    let (users, calls) = fake_users(&stop).await;
    let auth = fake_auth(&stop).await;
    let (posts_listener, posts) = listener().await;
    let posts_config = post_service::Config {
        users: users.to_string(),
        ..post_service::Config::default()
    };
    let post_task = tokio::spawn(post_service::run(
        posts_listener,
        posts_config,
        stop.clone(),
    ));
    let extra = Extra::start(Services {
        auth: auth.to_string(),
        auth_http: String::from("127.0.0.1:9"),
        users: users.to_string(),
        posts: posts.to_string(),
        chat: dead_addr().await,
        feed: dead_addr().await,
    })
    .await;
    let mut alice = extra.peer("alice-token").await;
    let post = ok!(alice, "posts.create", r#"{"text": "по следу"}"# => PostView);
    assert_eq!(
        post.author_name, "alice",
        "post-service спросил профиль автора у user-service"
    );
    let first = calls.take();
    let methods: Vec<&str> = first.iter().map(|(method, _)| method.as_str()).collect();
    assert!(
        methods.contains(&"GetProfile"),
        "на posts.create до user-service дошёл GetProfile: {first:?}"
    );
    let rid = &first[0].1;
    assert!(
        rid.len() == 16 && rid.chars().all(|c| c.is_ascii_hexdigit()),
        "x-request-id это 16 шестнадцатеричных знаков от шлюза: {first:?}"
    );
    assert!(
        first.iter().all(|(_, seen)| seen == rid),
        "один WS-запрос это один x-request-id во всех вызовах, и post-service пробрасывает его дальше: {first:?}"
    );
    let _ = ok!(alice, "posts.create", r#"{"text": "второй"}"# => PostView);
    let second = calls.take();
    assert!(
        !second.is_empty() && second.iter().all(|(_, seen)| seen != rid),
        "у следующего запроса свой x-request-id: {second:?}"
    );
    let leaky = alice.fails("users.find", r#"{"username": "leaky"}"#).await;
    assert_eq!(leaky.code, "internal", "{leaky:?}");
    assert!(
        !leaky.message.contains("hunter2") && !leaky.message.contains(LEAK),
        "текст внутренней ошибки сервиса не уходит клиенту: {leaky:?}"
    );
    let nobody = alice.fails("users.find", r#"{"username": "nobody"}"#).await;
    assert_eq!(
        (nobody.code.as_str(), nobody.message.as_str()),
        ("not_found", "пользователя @nobody нет"),
        "доменная ошибка проходит gRPC с кодом из x-error-code и своим текстом"
    );
    extra.stop().await;
    stop.cancel();
    let finished = within("остановка post-service", post_task)
        .await
        .expect("задача post-service не падает");
    assert!(finished.is_ok(), "{finished:?}");
}

#[tokio::test]
async fn a_fallen_service_is_unavailable_not_internal() {
    let stack = Gateway::start(Config::default()).await;
    let alice = stack.register("alice").await;
    let no_posts = Extra::start(Services {
        posts: dead_addr().await,
        ..stack.services.clone()
    })
    .await;
    let login = stack.login("alice").await;
    let mut peer = no_posts.peer(&login.token).await;
    let post = peer.fails("posts.create", r#"{"text": "в пустоту"}"#).await;
    assert!(
        post.code == "unavailable" && post.message.contains("сервис постов"),
        "post-service лежит: код unavailable и понятный текст, а не internal: {post:?}"
    );
    let found = ok!(peer, "users.find", r#"{"username": "alice"}"# => UserView);
    assert_eq!(
        found.id, alice.id,
        "остальные сервисы работают как ни в чём не бывало"
    );
    let (status, _) = raw(no_posts.addr, "GET", "/posts/1", &[], b"").await;
    assert_eq!(status, 503, "HTTP тоже отвечает 503");
    no_posts.stop().await;

    let no_feed = Extra::start(Services {
        feed: dead_addr().await,
        ..stack.services.clone()
    })
    .await;
    let mut reader = no_feed.peer(&login.token).await;
    let feed = reader.fails("posts.feed", "{}").await;
    assert!(
        feed.code == "unavailable" && feed.message.contains("сервис ленты"),
        "лента теперь у feed-service: он лежит, и posts.feed честно unavailable: {feed:?}"
    );
    no_feed.stop().await;

    let no_auth = Extra::start(Services {
        auth: dead_addr().await,
        ..stack.services.clone()
    })
    .await;
    let bearer = format!("Bearer {}", login.token);
    let (status, _) = raw(
        no_auth.addr,
        "GET",
        "/users/me",
        &[("Authorization", &bearer)],
        b"",
    )
    .await;
    assert_eq!(
        status, 503,
        "auth-service лежит: защищённый путь отвечает 503, а не 401, токен-то может быть живым"
    );
    let (status, _) = raw(
        no_auth.addr,
        "GET",
        &format!("/ws?token={}", login.token),
        &[
            ("Upgrade", "websocket"),
            ("Connection", "Upgrade"),
            ("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ=="),
            ("Sec-WebSocket-Version", "13"),
        ],
        b"",
    )
    .await;
    assert_eq!(status, 503, "и сокет без проверки токена не открывается");
    no_auth.stop().await;
    stack.stop().await;
}

#[tokio::test]
async fn big_avatar_goes_through_a_raised_limit() {
    let mib = 1024 * 1024;
    let stack = Gateway::start(Config {
        avatar_max: 6 * mib,
        http: ServerConfig {
            limits: Limits {
                max_body: 7 * mib,
                ..Limits::default()
            },
            ..ServerConfig::default()
        },
        ..Config::default()
    })
    .await;
    let alice = stack.register("alice").await;
    let login = stack.login("alice").await;
    let picture: Vec<u8> = (0..5 * mib).map(|i| (i % 251) as u8).collect();
    let bearer = format!("Bearer {}", login.token);
    let (status, _) = raw(
        stack.http,
        "PUT",
        "/users/me/avatar",
        &[("Authorization", &bearer), ("Content-Type", "image/png")],
        &picture,
    )
    .await;
    assert_eq!(
        status, 201,
        "аватар 5 МиБ больше стандартных 4 МиБ сообщения gRPC: лимит в user-service и в клиенте шлюза поднят осознанно"
    );
    let (status, body) = raw(
        stack.http,
        "GET",
        &format!("/avatars/{}", alice.id.0),
        &[],
        b"",
    )
    .await;
    assert!(
        status == 200 && body == picture,
        "аватар возвращается целиком: {status}, {} байт",
        body.len()
    );
    let channel = Channel::from_shared(format!("http://{}", stack.services.users))
        .expect("адрес user-service")
        .connect()
        .await
        .expect("gRPC user-service отвечает");
    let mut plain = UsersClient::with_interceptor(channel, Stamp::fresh().user(alice.id.0));
    let stored = plain
        .put_avatar(PutAvatarRequest {
            content_type: "image/png".to_string(),
            data: picture.clone(),
        })
        .await;
    assert!(
        stored.is_ok(),
        "сервер принимает 5 МиБ: max_decoding_message_size поднят на сервере: {stored:?}"
    );
    let fetched = plain
        .get_avatar(AvatarRequest {
            user_id: alice.id.0,
        })
        .await
        .expect_err("клиент со стандартным лимитом 4 МиБ не читает ответ в 5 МиБ");
    assert!(
        matches!(fetched.code(), Code::OutOfRange | Code::ResourceExhausted),
        "лимит есть с обеих сторон, поэтому клиент шлюза поднимает его тоже: {fetched:?}"
    );
    stack.stop().await;
}

#[tokio::test]
async fn each_service_keeps_its_own_journal() {
    let journals = Journals::fresh("m30", "journals");
    let stack = Gateway::with_journals(Config::default(), &journals).await;
    let (alice, mut writer) = stack.member("alice").await;
    let (_, mut reader) = stack.member("bob").await;
    let _ = ok!(reader, "users.follow", format!(r#"{{"target": "{}"}}"#, alice.id) => Changed);
    let post = ok!(writer, "posts.create", r#"{"text": "запомни меня"}"# => PostView);
    stack.stop().await;
    for (name, path, needle) in [
        ("user-service", &journals.users, "alice"),
        ("post-service", &journals.posts, "запомни меня"),
        ("auth-service", &journals.auth, "bob"),
    ] {
        let text = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("у {name} свой журнал {}: {e}", path.display()));
        assert!(
            text.contains(needle),
            "журнал {name} помнит «{needle}»:\n{text}"
        );
    }
    let stack = Gateway::with_journals(Config::default(), &journals).await;
    let mut reader = stack.device("bob").await;
    let feed = stack
        .feed_until(
            &mut reader,
            "пост alice после перезапуска",
            |page| !page.posts.is_empty(),
        )
        .await;
    assert_eq!(
        feed.posts.iter().map(|p| p.id).collect::<Vec<_>>(),
        vec![post.id],
        "после перезапуска bob входит (журнал auth-service) и видит пост alice (журналы post-service и user-service)"
    );
    let again = ok!(reader, "users.follow", format!(r#"{{"target": "{}"}}"#, alice.id) => Changed);
    assert!(!again.changed, "подписка пережила перезапуск user-service");
    stack.stop().await;
    journals.forget().await;
}

#[tokio::test]
async fn pushes_come_from_the_gateway_after_success() {
    let stack = Gateway::start(Config::default()).await;
    let (alice, mut author) = stack.member("alice").await;
    let (bob, mut fan) = stack.member("bob").await;
    let post = ok!(author, "posts.create", r#"{"text": "лайкни"}"# => PostView);
    let like = format!(r#"{{"post": "{}"}}"#, post.id);
    let first = ok!(fan, "posts.like", like.clone() => Changed);
    let second = ok!(fan, "posts.like", like.clone() => Changed);
    assert!(
        first.changed && !second.changed,
        "второй лайк ничего не меняет"
    );
    let own = ok!(author, "posts.like", like => Changed);
    assert!(own.changed, "свой пост лайкать можно");
    let unknown = fan.fails("users.follow", r#"{"target": "u_99"}"#).await;
    assert_eq!(unknown.code, "not_found", "{unknown:?}");
    let target = format!(r#"{{"target": "{}"}}"#, alice.id);
    let _ = ok!(fan, "users.follow", target.clone() => Changed);
    let _ = ok!(fan, "users.follow", target => Changed);
    let _ = ok!(fan, "chat.send", format!(r#"{{"peer": "{}", "text": "это всё"}}"#, alice.id) => MessageView);
    let mut seen = vec![
        plain(author.push().await),
        plain(author.push().await),
        plain(author.push().await),
    ];
    seen.sort_by_key(|push| format!("{push:?}"));
    let mut expected = vec![
        Push::Liked {
            to: alice.id,
            by: bob.id,
            post: post.id,
        },
        Push::Followed {
            to: alice.id,
            by: bob.id,
        },
        letter(alice.id, bob.id, "это всё"),
    ];
    expected.sort_by_key(|push| format!("{push:?}"));
    assert_eq!(
        seen, expected,
        "пуш уходит только после успешного вызова и только если что-то изменилось: \
         ни второго лайка, ни своего лайка, ни подписки на несуществующего, ни повторной подписки; \
         лайк и подписка идут событием через notify-service, сообщение через stream чата, \
         поэтому порядок между ними не обещан"
    );
    stack.stop().await;
}
