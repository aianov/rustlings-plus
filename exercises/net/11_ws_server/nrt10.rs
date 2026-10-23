// Запросы приходят как «сервис, метод, данные», а обрабатывает их по одной функции на метод. Пока соединение одно, `match` на строку
// терпимо, но методов будет сотня, и каждому нужно решить, пускать ли без входа. Заведи таблицу: имя вида "post.like" ведёт
// к асинхронному обработчику. Часть методов открыта всем (`route`), часть требует пользователя (`route_authed`). Пользователя
// обработчик получает от сервера, из сессии соединения, а не из присланных данных: клиент может написать в data что угодно.
// TODO: Допиши Router.

#![allow(dead_code)]
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

type UserId = u64;
type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

#[derive(Debug, PartialEq)]
struct ApiError {
    code: &'static str,
    message: String,
}

type Reply = Result<Value, ApiError>;
type OpenHandler = Arc<dyn Fn(Value) -> BoxFuture<Reply> + Send + Sync>;
type AuthedHandler = Arc<dyn Fn(UserId, Value) -> BoxFuture<Reply> + Send + Sync>;

#[derive(Clone)]
enum Route {
    Open(OpenHandler),
    Authed(AuthedHandler),
}

struct Session {
    user: Option<UserId>,
}

#[derive(Default)]
struct Router {
    routes: HashMap<String, Route>,
}

impl Router {
    fn new() -> Router {
        Router::default()
    }

    fn route<F, Fut>(self, name: &str, handler: F) -> Router
    where
        F: Fn(Value) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Reply> + Send + 'static,
    {
        todo!()
    }

    fn route_authed<F, Fut>(self, name: &str, handler: F) -> Router
    where
        F: Fn(UserId, Value) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Reply> + Send + 'static,
    {
        todo!()
    }

    async fn dispatch(&self, session: &Session, service: &str, method: &str, data: Value) -> Reply {
        let _ = (session, service, method, data);
        todo!()
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn anon() -> Session {
        Session { user: None }
    }

    fn user(id: UserId) -> Session {
        Session { user: Some(id) }
    }

    #[tokio::test]
    async fn open_route_is_called_with_data() {
        let router = Router::new().route("echo.say", |data| async move { Ok(json!({ "you_said": data })) });
        let reply = router.dispatch(&anon(), "echo", "say", json!("привет")).await;
        assert_eq!(reply, Ok(json!({ "you_said": "привет" })));
    }

    #[tokio::test]
    async fn unknown_method_is_not_found_and_name_is_exact() {
        let router = Router::new().route("echo.say", |_| async { Ok(Value::Null) });
        for (service, method) in [("echo", "shout"), ("other", "say"), ("echo.say", ""), ("", "echo.say"), ("ECHO", "say")] {
            let err = router.dispatch(&user(1), service, method, Value::Null).await.unwrap_err();
            assert_eq!(err.code, "not_found", "{service:?}.{method:?} не зарегистрирован");
        }
    }

    #[tokio::test]
    async fn authed_route_needs_a_session_user_and_does_not_run_without_one() {
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        let router = Router::new().route_authed("post.like", move |user, data| {
            let counter = counter.clone();
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(json!({ "user": user, "post": data["post"] }))
            }
        });
        let err = router.dispatch(&anon(), "post", "like", json!({ "post": 3 })).await.unwrap_err();
        assert_eq!(err.code, "unauthorized");
        assert_eq!(calls.load(Ordering::SeqCst), 0, "без входа обработчик вызываться не должен вообще");
        let ok = router.dispatch(&user(12), "post", "like", json!({ "post": 3 })).await.unwrap();
        assert_eq!(ok, json!({ "user": 12, "post": 3 }));
    }

    #[tokio::test]
    async fn user_comes_from_the_session_not_from_data() {
        let router = Router::new().route_authed("user.whoami", |user, _data| async move { Ok(json!(user)) });
        let forged = json!({ "user": 999, "user_id": 999, "id": 999 });
        let reply = router.dispatch(&user(5), "user", "whoami", forged).await.unwrap();
        assert_eq!(reply, json!(5), "обработчик получил пользователя из присланных данных вместо сессии");
    }

    #[tokio::test]
    async fn handler_errors_pass_through_and_state_is_shared() {
        let hits = Arc::new(AtomicUsize::new(0));
        let h = hits.clone();
        let router = Router::new()
            .route("chat.fail", |_| async { Err(ApiError { code: "too_long", message: "слишком длинно".into() }) })
            .route("chat.count", move |_| {
                let h = h.clone();
                async move { Ok(json!(h.fetch_add(1, Ordering::SeqCst) + 1)) }
            });
        let err = router.dispatch(&anon(), "chat", "fail", Value::Null).await.unwrap_err();
        assert_eq!((err.code, err.message.as_str()), ("too_long", "слишком длинно"));
        for expected in 1..=3 {
            assert_eq!(router.dispatch(&anon(), "chat", "count", Value::Null).await, Ok(json!(expected)));
        }
    }

    #[test]
    #[should_panic(expected = "дважды")]
    fn registering_a_name_twice_is_a_startup_bug() {
        let _ = Router::new().route("a.b", |_| async { Ok(Value::Null) }).route_authed("a.b", |_, _| async { Ok(Value::Null) });
    }
}
