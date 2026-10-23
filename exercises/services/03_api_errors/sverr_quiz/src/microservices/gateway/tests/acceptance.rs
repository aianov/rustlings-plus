use contracts::api::{PostApi, User, UserApi};
use contracts::errors::{GrpcCode, GrpcStatus};
use gateway::{Gateway, HttpRequest};
use post_service::controller::PostController;
use post_service::domain::PostService;
use std::sync::{Arc, Mutex};
use user_service::controller::UserController;
use user_service::domain::UserStore;

type Log = Arc<Mutex<Vec<String>>>;

struct Scripted(Box<dyn Fn(u64) -> Result<User, GrpcStatus> + Send + Sync>);

impl UserApi for Scripted {
    fn get(&self, id: u64, _: &str) -> Result<User, GrpcStatus> {
        (self.0)(id)
    }
}

fn alice() -> User {
    User { id: 7, name: "alice".to_string() }
}

fn real_stack(store: UserStore) -> (Gateway, Log) {
    let users: Arc<dyn UserApi> = Arc::new(UserController::new(store));
    let posts: Arc<dyn PostApi> = Arc::new(PostController::new(PostService::new(users.clone())));
    let log: Log = Arc::default();
    (Gateway::new(users, posts, log.clone()), log)
}

fn stack_with_users(users: Arc<dyn UserApi>) -> (Gateway, Log) {
    let posts: Arc<dyn PostApi> = Arc::new(PostController::new(PostService::new(users.clone())));
    let log: Log = Arc::default();
    (Gateway::new(users, posts, log.clone()), log)
}

fn req(method: &str, path: &str, body: &str) -> HttpRequest {
    HttpRequest { id: "req-1".to_string(), method: method.to_string(), path: path.to_string(), body: body.as_bytes().to_vec() }
}

fn code_of(reply: &contracts::errors::HttpReply) -> String {
    reply.json()["error"]["code"].as_str().unwrap_or("<нет кода>").to_string()
}

#[test]
fn happy_paths() {
    let (gateway, _) = real_stack(UserStore::with_users(vec![alice()]));
    let reply = gateway.handle(req("GET", "/users/7", ""));
    assert_eq!(reply.status, 200, "пользователь найден");
    let reply = gateway.handle(req("POST", "/posts", r#"{"author_id":7,"text":"привет"}"#));
    assert_eq!(reply.status, 200, "пост создан: {}", reply.body);
    assert_eq!(reply.json()["author_id"], 7, "в ответе автор");
}

#[test]
fn missing_user_is_404_with_a_stable_code() {
    let (gateway, _) = real_stack(UserStore::with_users(vec![]));
    let reply = gateway.handle(req("GET", "/users/9", ""));
    assert_eq!(reply.status, 404, "нет пользователя");
    assert_eq!(code_of(&reply), "user.not_found", "стабильный код");
    assert_eq!(reply.json()["error"]["request_id"], "req-1", "номер запроса шлюза");
}

#[test]
fn missing_author_of_a_post_is_a_request_problem_not_404() {
    let (gateway, _) = real_stack(UserStore::with_users(vec![]));
    let reply = gateway.handle(req("POST", "/posts", r#"{"author_id":9,"text":"привет"}"#));
    assert_eq!(reply.status, 422, "ошибка прошла три перехода: user-service NotFound, post-service AuthorNotFound, шлюз 422: {}", reply.body);
    assert_eq!(code_of(&reply), "post.author_not_found", "код, который понятен клиенту");
}

#[test]
fn storage_failures_look_the_same_on_every_route_and_reveal_nothing() {
    let (gateway, log) = real_stack(UserStore::broken("postgres://app:s3cr3t@db-7.internal/users: reset"));
    for reply in [
        gateway.handle(req("GET", "/users/7", "")),
        gateway.handle(req("POST", "/posts", r#"{"author_id":7,"text":"привет"}"#)),
    ] {
        assert_eq!(reply.status, 500, "сломанное хранилище это 500");
        assert_eq!(code_of(&reply), "internal", "код internal");
        let sent = format!("{:?} {}", reply.headers, reply.body);
        assert!(!sent.contains("s3cr3t") && !sent.contains("db-7"), "клиент увидел внутренности: {sent}");
    }
    assert!(!log.lock().unwrap().is_empty(), "а в журнал шлюза запись о сбое попала");
}

#[test]
fn neighbour_without_rights_is_our_bug_not_a_403() {
    let users = Arc::new(Scripted(Box::new(|_| Err(GrpcStatus::new(GrpcCode::PermissionDenied, "service token expired")))));
    let (gateway, _) = stack_with_users(users);
    for reply in [
        gateway.handle(req("GET", "/users/7", "")),
        gateway.handle(req("POST", "/posts", r#"{"author_id":7,"text":"привет"}"#)),
    ] {
        assert_eq!(reply.status, 500, "отказ в правах при вызове сервис-к-сервису не повод говорить клиенту «нет прав»: {}", reply.body);
        assert!(!reply.body.contains("token"), "текст соседа клиенту не показывается: {}", reply.body);
    }
}

#[test]
fn unavailable_neighbour_gives_503_with_a_pause() {
    let users = Arc::new(Scripted(Box::new(|_| {
        Err(GrpcStatus::new(GrpcCode::Unavailable, "connection refused").with_meta("retry-after", "2"))
    })));
    let (gateway, _) = stack_with_users(users);
    let reply = gateway.handle(req("POST", "/posts", r#"{"author_id":7,"text":"привет"}"#));
    assert_eq!(reply.status, 503, "сосед недоступен");
    assert_eq!(code_of(&reply), "service.unavailable", "код недоступности");
    assert!(reply.header("retry-after").is_some(), "клиенту сказано, когда повторить: {:?}", reply.headers);
}

#[test]
fn slow_neighbour_gives_504() {
    let users = Arc::new(Scripted(Box::new(|_| Err(GrpcStatus::new(GrpcCode::DeadlineExceeded, "deadline")))));
    let (gateway, _) = stack_with_users(users);
    let reply = gateway.handle(req("GET", "/users/7", ""));
    assert_eq!((reply.status, code_of(&reply).as_str()), (504, "service.timeout"), "сосед не успел: {}", reply.body);
}

#[test]
fn broken_json_is_400_and_wrong_fields_are_422() {
    let (gateway, _) = real_stack(UserStore::with_users(vec![alice()]));
    for body in ["", "{", "привет"] {
        let reply = gateway.handle(req("POST", "/posts", body));
        assert_eq!((reply.status, code_of(&reply).as_str()), (400, "request.malformed"), "не JSON: {body:?}");
    }
    let reply = gateway.handle(req("POST", "/posts", r#"{"author_id":"hunter2hunter2","text":""}"#));
    assert_eq!((reply.status, code_of(&reply).as_str()), (422, "validation.failed"), "поля не годятся: {}", reply.body);
    let details = reply.json()["error"]["details"].clone();
    assert_eq!(
        details,
        serde_json::json!([{"field": "author_id", "code": "required"}, {"field": "text", "code": "required"}]),
        "оба нарушения сразу"
    );
    assert!(!reply.body.contains("hunter2"), "присланное значение не возвращается: {}", reply.body);
}

#[test]
fn too_long_text_is_422_with_a_field() {
    let (gateway, _) = real_stack(UserStore::with_users(vec![alice()]));
    let body = format!(r#"{{"author_id":7,"text":"{}"}}"#, "я".repeat(281));
    let reply = gateway.handle(req("POST", "/posts", &body));
    assert_eq!(reply.status, 422, "слишком длинный пост");
    assert_eq!(reply.json()["error"]["details"], serde_json::json!([{"field": "text", "code": "too_long"}]), "поле и причина дошли через два сервиса");
}

#[test]
fn a_panic_in_a_neighbour_is_a_500_and_the_gateway_lives_on() {
    let users = Arc::new(Scripted(Box::new(|id| {
        if id == 13 {
            panic!("боевой секрет: пароль hunter2");
        }
        Ok(alice())
    })));
    let (gateway, log) = stack_with_users(users);
    let reply = gateway.handle(req("GET", "/users/13", ""));
    assert_eq!(reply.status, 500, "паника превращается в 500");
    assert_eq!(code_of(&reply), "internal", "код internal");
    assert_eq!(reply.json()["error"]["request_id"], "req-1", "номер запроса на месте");
    assert!(!reply.body.contains("hunter2"), "текст паники клиенту не уходит: {}", reply.body);
    assert!(log.lock().unwrap().iter().any(|l| l.contains("hunter2") && l.contains("req-1")), "а в журнале он есть вместе с номером запроса");
    assert_eq!(gateway.handle(req("GET", "/users/7", "")).status, 200, "следующий запрос обслуживается");
}

#[test]
fn unknown_route_is_a_clients_mistake() {
    let (gateway, _) = real_stack(UserStore::with_users(vec![]));
    let reply = gateway.handle(req("GET", "/users/abc", ""));
    assert_eq!(reply.status, 400, "id не число: {}", reply.body);
}
