use post_service::bootstrap::build_with;
use post_service::controller::PostController;
use post_service::domain::{DirectoryAnswer, Post, PostError, UserDirectory};
use post_service::errors::{client_error, status_from};
use post_service::pb::posts_client::PostsClient;
use post_service::pb::posts_server::{Posts, PostsServer};
use post_service::pb::{CreatePostRequest, GetPostRequest};
use std::sync::Arc;
use tonic::{Code, Request, Status};

struct Directory(fn(u64) -> DirectoryAnswer);

impl UserDirectory for Directory {
    fn lookup(&self, user_id: u64) -> DirectoryAnswer {
        (self.0)(user_id)
    }
}

fn controller(answer: fn(u64) -> DirectoryAnswer) -> PostController {
    let seed = vec![Post { id: 1, author_id: 7, text: "первый".to_string() }];
    build_with(Arc::new(Directory(answer)), seed)
}

fn everyone_exists(_: u64) -> DirectoryAnswer {
    DirectoryAnswer::Exists
}

fn with_id<T>(message: T, id: &str) -> Request<T> {
    let mut request = Request::new(message);
    request.metadata_mut().insert("x-request-id", id.parse().unwrap());
    request
}

fn meta(status: &Status, key: &str) -> Option<String> {
    status.metadata().get(key).map(|v| v.to_str().unwrap().to_string())
}

#[tokio::test]
async fn not_found_carries_a_machine_code_and_the_request_id() {
    let status = controller(everyone_exists).get_post(with_id(GetPostRequest { id: 404 }, "req-1")).await.unwrap_err();
    assert_eq!(status.code(), Code::NotFound, "нет поста это NotFound");
    assert_eq!(meta(&status, "x-error-code").as_deref(), Some("post.not_found"), "машинный код в метаданных x-error-code");
    assert_eq!(meta(&status, "x-request-id").as_deref(), Some("req-1"), "номер запроса клиента возвращается в ответе");
    assert!(status.message().contains("404"), "человеческий текст называет пост: {}", status.message());
}

#[tokio::test]
async fn request_id_is_generated_when_the_client_sent_none() {
    let c = controller(everyone_exists);
    let first = c.get_post(Request::new(GetPostRequest { id: 404 })).await.unwrap_err();
    let second = c.get_post(Request::new(GetPostRequest { id: 404 })).await.unwrap_err();
    let (a, b) = (meta(&first, "x-request-id"), meta(&second, "x-request-id"));
    assert!(a.as_deref().is_some_and(|s| !s.is_empty()), "без номера от клиента сервис выдаёт свой");
    assert_ne!(a, b, "у разных запросов разные номера");
}

#[tokio::test]
async fn create_errors_are_told_apart() {
    fn missing(_: u64) -> DirectoryAnswer {
        DirectoryAnswer::Missing
    }
    fn down(_: u64) -> DirectoryAnswer {
        DirectoryAnswer::Down
    }
    let create = |author_id: u64, text: &str| CreatePostRequest { author_id, text: text.to_string() };

    let status = controller(missing).create_post(Request::new(create(9, "привет"))).await.unwrap_err();
    assert_eq!(status.code(), Code::InvalidArgument, "автора нет: ошибка в запросе, а не «нет ресурса»");
    assert_eq!(meta(&status, "x-error-code").as_deref(), Some("post.author_not_found"), "код автора");

    let long = "я".repeat(281);
    let status = controller(everyone_exists).create_post(Request::new(create(7, &long))).await.unwrap_err();
    assert_eq!(status.code(), Code::InvalidArgument, "слишком длинный текст");
    assert_eq!(meta(&status, "x-error-code").as_deref(), Some("validation.failed"), "код проверки");
    assert_eq!(meta(&status, "x-error-details").as_deref(), Some("text:too_long"), "поле и причина в x-error-details");

    let status = controller(down).create_post(Request::new(create(7, "привет"))).await.unwrap_err();
    assert_eq!(status.code(), Code::Unavailable, "справочник недоступен");
    assert_eq!(meta(&status, "x-error-code").as_deref(), Some("service.unavailable"), "код недоступности");
    assert_eq!(meta(&status, "retry-after").as_deref(), Some("1"), "через сколько секунд пробовать снова");
}

#[tokio::test]
async fn internal_errors_do_not_leak() {
    fn broken(_: u64) -> DirectoryAnswer {
        DirectoryAnswer::Broken("postgres://app:s3cr3t@db-7.internal/users: connection reset".to_string())
    }
    let request = CreatePostRequest { author_id: 7, text: "привет".to_string() };
    let status = controller(broken).create_post(with_id(request, "req-5")).await.unwrap_err();
    assert_eq!(status.code(), Code::Internal, "внутренняя ошибка");
    assert_eq!(meta(&status, "x-error-code").as_deref(), Some("internal"), "код internal");
    let shown = format!("{} {:?}", status.message(), status.metadata());
    assert!(!shown.contains("s3cr3t") && !shown.contains("db-7"), "строка подключения ушла клиенту: {shown}");
    assert_eq!(meta(&status, "x-request-id").as_deref(), Some("req-5"), "а номер запроса остался");
}

#[test]
fn status_from_is_usable_without_a_server() {
    let status = status_from(&PostError::NotFound(3), "r-1");
    assert_eq!(status.code(), Code::NotFound, "NotFound");
    let status = status_from(&PostError::Internal("secret".into()), "r-2");
    assert!(!status.message().contains("secret"), "общее сообщение: {}", status.message());
}

#[test]
fn client_side_reads_the_machine_code_back() {
    let status = status_from(&PostError::TextTooLong { len: 300, max: 280 }, "r-3");
    let err = client_error(&status);
    assert_eq!(err.code, "validation.failed", "код достаётся из метаданных");
    assert_eq!(err.request_id.as_deref(), Some("r-3"), "номер запроса для жалобы");
    assert_eq!(err.details, vec![("text".to_string(), "too_long".to_string())], "детали разобраны на пары");
    assert!(!err.retryable, "повтор того же текста не поможет");

    let err = client_error(&status_from(&PostError::Unavailable, "r-4"));
    assert!(err.retryable, "Unavailable можно повторять");
    assert_eq!(err.retry_after_secs, Some(1), "и подсказано, через сколько");
}

#[test]
fn client_survives_statuses_it_has_never_seen() {
    let from_proxy = Status::unavailable("upstream connect error or disconnect/reset before headers");
    let err = client_error(&from_proxy);
    assert_eq!(err.code, "service.unavailable", "статус без наших метаданных (его сгенерировал прокси) разбирается по gRPC-коду");
    assert!(err.retryable, "Unavailable повторяем и в этом случае");
    assert_eq!(err.request_id, None, "номера запроса нет");

    let mut future = Status::new(Code::FailedPrecondition, "архив");
    future.metadata_mut().insert("x-error-code", "post.archived".parse().unwrap());
    let err = client_error(&future);
    assert_eq!(err.code, "post.archived", "новый код сервера сохраняется как строка");
    assert!(!err.retryable, "FailedPrecondition не повторяют");

    let err = client_error(&Status::new(Code::Unknown, "что-то"));
    assert_eq!(err.code, "internal", "совсем неизвестное считается внутренней ошибкой");
}

#[tokio::test]
async fn metadata_survives_the_wire() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let service = PostsServer::new(controller(everyone_exists));
    tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(service)
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
    });

    let mut client = PostsClient::connect(format!("http://{addr}")).await.expect("подключение к серверу");
    let status = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        client.get_post(with_id(GetPostRequest { id: 999 }, "wire-1")),
    )
    .await
    .expect("ответ не пришёл")
    .unwrap_err();
    let err = client_error(&status);
    assert_eq!(err.code, "post.not_found", "машинный код дошёл до клиента по сети: метаданные ошибки едут в trailers");
    assert_eq!(err.request_id.as_deref(), Some("wire-1"), "номер запроса вернулся тем же");

    let ok = client.get_post(GetPostRequest { id: 1 }).await.expect("пост 1 есть").into_inner();
    assert_eq!(ok.text, "первый", "успешные вызовы работают как прежде");
}
