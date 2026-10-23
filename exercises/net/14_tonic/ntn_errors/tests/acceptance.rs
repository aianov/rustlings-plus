use ntn_errors::pb::users_client::UsersClient;
use ntn_errors::pb::{CreateUserRequest, GetUserRequest, User};
use ntn_errors::{start, Call, DomainError, Journal, Repo, UsersSvc};
use tonic::Code;

async fn within<T>(what: &str, f: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(std::time::Duration::from_secs(5), f)
        .await
        .unwrap_or_else(|_| panic!("{what}: не дождались ответа за 5 секунд"))
}

const SECRET: &str = "connection to postgres://admin:s3cret@10.0.0.5/social refused";

fn repo() -> Repo {
    Box::new(|call| match call {
        Call::Get(7) => Ok(User { id: 7, handle: "ann".to_string(), display_name: String::new() }),
        Call::Get(13) => Err(DomainError::Db(SECRET.to_string())),
        Call::Get(id) => Err(DomainError::NotFound(id)),
        Call::Create(h) if h == "taken" => Err(DomainError::HandleTaken(h)),
        Call::Create(h) if h == "bad!" => Err(DomainError::InvalidHandle(h)),
        Call::Create(_) => Err(DomainError::Db(SECRET.to_string())),
    })
}

async fn client() -> (UsersClient<tonic::transport::Channel>, Journal) {
    let journal = Journal::default();
    let addr = start(UsersSvc::new(repo(), journal.clone())).await.unwrap();
    (UsersClient::connect(format!("http://{addr}")).await.unwrap(), journal)
}

#[tokio::test]
async fn normal_answers_still_work() {
    let (mut client, _) = client().await;
    let user = within("get", client.get_user(GetUserRequest { id: 7 })).await.unwrap().into_inner();
    assert_eq!(user.handle, "ann");
}

#[tokio::test]
async fn client_visible_errors_keep_their_codes() {
    let (mut client, _) = client().await;
    let err = within("get", client.get_user(GetUserRequest { id: 99 })).await.unwrap_err();
    assert_eq!(err.code(), Code::NotFound, "нет пользователя это NotFound");
    assert!(err.message().contains("99"), "для NotFound сообщение должно называть номер: {}", err.message());
    let err = within("create", client.create_user(CreateUserRequest { handle: "taken".into(), display_name: String::new() })).await.unwrap_err();
    assert_eq!(err.code(), Code::AlreadyExists);
    let err = within("create", client.create_user(CreateUserRequest { handle: "bad!".into(), display_name: String::new() })).await.unwrap_err();
    assert_eq!(err.code(), Code::InvalidArgument);
}

#[tokio::test]
async fn database_errors_do_not_leak_to_the_client() {
    let (mut client, _) = client().await;
    let err = within("get", client.get_user(GetUserRequest { id: 13 })).await.unwrap_err();
    assert_eq!(err.code(), Code::Internal, "поломка базы это Internal");
    for secret in ["s3cret", "postgres", "10.0.0.5", "admin", "refused"] {
        assert!(!err.message().contains(secret), "клиент увидел {secret:?} в сообщении об ошибке: {}", err.message());
    }
    assert!(!err.message().is_empty(), "сообщение не должно быть пустым: клиенту нужно хоть что-то сказать");
}

#[tokio::test]
async fn full_text_stays_in_the_journal() {
    let (mut client, journal) = client().await;
    let _ = within("create", client.create_user(CreateUserRequest { handle: "boom".into(), display_name: String::new() })).await;
    let lines = journal.lock().unwrap().clone();
    assert!(lines.iter().any(|l| l.contains("s3cret")), "в журнале должен остаться полный текст ошибки, иначе поломку нечем разбирать: {lines:?}");
}
