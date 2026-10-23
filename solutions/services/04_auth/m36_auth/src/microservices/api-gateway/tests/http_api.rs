use std::net::SocketAddr;

use api_gateway::{Config, Services, run};
use contracts::{Credentials, Login, UserView, Username};
use http_client::{Client, Error, Request};
use service_runtime::CancellationToken;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

struct Running {
    base: String,
    auth: String,
    stop: CancellationToken,
    tasks: Vec<JoinHandle<Result<(), String>>>,
}

async fn listener() -> (TcpListener, SocketAddr) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("свободный порт есть");
    let addr = listener.local_addr().expect("у слушателя есть адрес");
    (listener, addr)
}

async fn start() -> Running {
    let (users, users_addr) = listener().await;
    let (auth_http, auth_addr) = listener().await;
    let (auth_grpc, auth_grpc_addr) = listener().await;
    let (http, addr) = listener().await;
    let stop = CancellationToken::new();
    let users_config = user_service::Config::default();
    let auth_config = auth_service::Config {
        users: users_addr.to_string(),
        ..auth_service::Config::default()
    };
    let config = Config {
        services: Services {
            auth: auth_grpc_addr.to_string(),
            users: users_addr.to_string(),
            posts: String::from("127.0.0.1:9"),
            chat: String::from("127.0.0.1:9"),
        },
        ..Config::default()
    };
    let users = user_service::run(users, users_config, stop.clone());
    let auth = auth_service::run(auth_http, auth_grpc, auth_config, stop.clone());
    let gateway = run(http, config, stop.clone());
    let tasks = vec![
        tokio::spawn(async move { gateway.await.map_err(|e| format!("{e:?}")) }),
        tokio::spawn(async move { auth.await.map_err(|e| format!("{e:?}")) }),
        tokio::spawn(async move { users.await.map_err(|e| format!("{e:?}")) }),
    ];
    Running {
        base: format!("http://{addr}"),
        auth: format!("http://{auth_addr}"),
        stop,
        tasks,
    }
}

impl Running {
    async fn stop(self) {
        self.stop.cancel();
        for task in self.tasks {
            let finished = task.await.expect("задача сервиса не падает");
            assert!(
                finished.is_ok(),
                "сервис останавливается без ошибки: {finished:?}"
            );
        }
    }
}

fn credentials(name: &str) -> Credentials {
    Credentials {
        username: Username::try_from(name).expect("ник корректный"),
        password: String::from("correct-horse"),
    }
}

#[tokio::test]
async fn register_login_and_me() {
    let gateway = start().await;
    let client = Client::new(http_client::Config::default());
    let (base, auth) = (&gateway.base, &gateway.auth);
    let alice: UserView = client
        .post_json(&format!("{auth}/auth/register"), &credentials("alice"))
        .await
        .expect("регистрация");
    let login: Login = client
        .post_json(&format!("{auth}/auth/login"), &credentials("alice"))
        .await
        .expect("вход");
    assert_eq!(login.user, alice);

    let me = client
        .send(
            Request::get(&format!("{base}/users/me"))
                .header("Authorization", &format!("Bearer {}", login.token)),
        )
        .await
        .expect("запрос /users/me");
    assert_eq!(me.status, 200);
    assert!(me.text().contains("\"alice\""), "{}", me.text());

    let anonymous = client
        .get_json::<UserView>(&format!("{base}/users/me"))
        .await;
    assert!(
        matches!(anonymous, Err(Error::Status { code: 401, .. })),
        "{anonymous:?}"
    );
    assert_eq!(
        client.idle_connections(),
        2,
        "по одному keep-alive соединению на auth-service и на шлюз"
    );
    gateway.stop().await;
}

#[tokio::test]
async fn second_registration_is_a_conflict() {
    let gateway = start().await;
    let client = Client::new(http_client::Config::default());
    let url = format!("{}/auth/register", gateway.auth);
    let _: UserView = client
        .post_json(&url, &credentials("bob"))
        .await
        .expect("первая регистрация");
    let again = client
        .post_json::<_, UserView>(&url, &credentials("bob"))
        .await;
    assert!(
        matches!(again, Err(Error::Status { code: 409, .. })),
        "{again:?}"
    );
    gateway.stop().await;
}
