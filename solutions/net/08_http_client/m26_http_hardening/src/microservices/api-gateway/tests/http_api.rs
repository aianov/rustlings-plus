use std::net::SocketAddr;

use api_gateway::{Config, run};
use contracts::{Credentials, Login, UserView, Username};
use http_client::{Client, Error, Request};
use service_runtime::CancellationToken;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

struct Running {
    base: String,
    stop: CancellationToken,
    task: JoinHandle<api_gateway::Result<()>>,
}

async fn listener() -> (TcpListener, SocketAddr) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("свободный порт есть");
    let addr = listener.local_addr().expect("у слушателя есть адрес");
    (listener, addr)
}

async fn start() -> Running {
    let (tcp, _) = listener().await;
    let (http, addr) = listener().await;
    let stop = CancellationToken::new();
    let task = tokio::spawn(run(tcp, http, Config::default(), stop.clone()));
    Running {
        base: format!("http://{addr}"),
        stop,
        task,
    }
}

impl Running {
    async fn stop(self) {
        self.stop.cancel();
        let finished = self.task.await.expect("задача шлюза не падает");
        assert!(finished.is_ok(), "шлюз останавливается без ошибки");
    }
}

fn credentials(name: &str) -> Credentials {
    Credentials {
        username: Username::try_from(name).expect("ник корректный"),
    }
}

#[tokio::test]
async fn register_login_and_me() {
    let gateway = start().await;
    let client = Client::new(http_client::Config::default());
    let base = &gateway.base;
    let alice: UserView = client
        .post_json(&format!("{base}/auth/register"), &credentials("alice"))
        .await
        .expect("регистрация");
    let login: Login = client
        .post_json(&format!("{base}/auth/login"), &credentials("alice"))
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
        1,
        "всё по одному keep-alive соединению"
    );
    gateway.stop().await;
}

#[tokio::test]
async fn second_registration_is_a_conflict() {
    let gateway = start().await;
    let client = Client::new(http_client::Config::default());
    let url = format!("{}/auth/register", gateway.base);
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
