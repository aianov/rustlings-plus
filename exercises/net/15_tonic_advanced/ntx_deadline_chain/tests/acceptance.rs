use ntx_deadline_chain::posts::v1::posts_client::PostsClient;
use ntx_deadline_chain::posts::v1::GetPostRequest;
use ntx_deadline_chain::users::v1::users_server::{Users, UsersServer};
use ntx_deadline_chain::users::v1::{CreateUserRequest, GetUserRequest, User};
use ntx_deadline_chain::{parse_grpc_timeout, start};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::{Channel, Endpoint, Server};
use tonic::{Code, Request, Response, Status};

async fn within<T>(what: &str, f: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(std::time::Duration::from_secs(5), f)
        .await
        .unwrap_or_else(|_| panic!("{what}: не дождались ответа за 5 секунд"))
}

#[derive(Clone)]
struct Users_ {
    delay: Duration,
    seen: Arc<Mutex<Vec<Option<Duration>>>>,
}

#[tonic::async_trait]
impl Users for Users_ {
    async fn get_user(&self, request: Request<GetUserRequest>) -> Result<Response<User>, Status> {
        let header = request.metadata().get("grpc-timeout").and_then(|v| v.to_str().ok()).and_then(parse_grpc_timeout);
        self.seen.lock().unwrap().push(header);
        tokio::time::sleep(self.delay).await;
        Ok(Response::new(User { id: request.into_inner().id, handle: "ann".to_string(), display_name: String::new() }))
    }
    async fn create_user(&self, _: Request<CreateUserRequest>) -> Result<Response<User>, Status> {
        Err(Status::unimplemented("не нужен"))
    }
}

async fn chain(users_delay: Duration) -> (PostsClient<Channel>, Arc<Mutex<Vec<Option<Duration>>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let users = Users_ { delay: users_delay, seen: seen.clone() };
    tokio::spawn(Server::builder().add_service(UsersServer::new(users)).serve_with_incoming(TcpListenerStream::new(listener)));
    let users_channel = Endpoint::from_shared(format!("http://{addr}")).unwrap().connect_lazy();
    let posts = start(users_channel).await.unwrap();
    (PostsClient::new(Endpoint::from_shared(format!("http://{posts}")).unwrap().connect_lazy()), seen)
}

fn with_budget(ms: u64) -> Request<GetPostRequest> {
    let mut request = Request::new(GetPostRequest { id: 1 });
    request.set_timeout(Duration::from_millis(ms));
    request
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

#[tokio::test]
async fn users_gets_what_is_left_of_a_roomy_budget() {
    let (mut posts, seen) = chain(ms(0)).await;
    within("вызов", posts.get_post(with_budget(600))).await.unwrap();
    let got = seen.lock().unwrap()[0].expect("users не получил срока вообще");
    assert!(got >= ms(350) && got <= ms(451), "бюджет 600 мс минус 150 мс работы должен дать users около 450 мс, а users получил {got:?}");
}

#[tokio::test]
async fn tight_budget_squeezes_the_upstream_call() {
    let (mut posts, seen) = chain(ms(300)).await;
    let started = Instant::now();
    let err = within("вызов", posts.get_post(with_budget(220))).await.expect_err("users медленный, бюджет мал: вызов обязан оборваться");
    assert!(matches!(err.code(), Code::DeadlineExceeded | Code::Cancelled), "ждали «не успели», получили {err}");
    assert!(started.elapsed() < ms(450), "вызов тянулся {:?}: бюджет 220 мс", started.elapsed());
    let got = seen.lock().unwrap().first().copied().flatten().expect("users должен был получить вызов со сроком");
    assert!(got >= ms(15) && got <= ms(71), "от бюджета 220 мс после 150 мс работы остаётся около 70 мс, а users получил {got:?}");
}

#[tokio::test]
async fn call_without_budget_still_gets_a_ceiling() {
    let (mut posts, seen) = chain(ms(0)).await;
    within("вызов", posts.get_post(GetPostRequest { id: 1 })).await.unwrap();
    let got = seen.lock().unwrap()[0].expect("вызов users без срока: в цепочке у всех вызовов должен быть потолок");
    assert!(got >= ms(900) && got <= ms(1000), "без входящего срока users должен получить потолок DEFAULT_UPSTREAM (1 с), а получил {got:?}");
}

#[tokio::test]
async fn fast_scenario_returns_the_author() {
    let (mut posts, _seen) = chain(ms(0)).await;
    let post = within("вызов", posts.get_post(with_budget(2000))).await.unwrap().into_inner();
    assert_eq!(post.author.map(|a| a.handle), Some("ann".to_string()));
}
