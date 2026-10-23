// Метод создания поста в шлюзе читает автора из контекста: `ctx.principal` это `Option`, и разработчик про
// `None` забыл. Аноним теперь публикует посты от «пустого» пользователя. Чинить по одному `if` в каждом обработчике
// бесполезно: забудут снова. Правильнее, чтобы защищённый маршрут регистрировался отдельно (`route_authed`), его
// обработчик получал `Principal` без `Option`, а проверку делал сам шлюз. Цель: `route_authed` отвечает анониму
// `unauthenticated` и не вызывает обработчик; при этом данные запроса анонима даже не разбираются
// (иначе по ошибкам разбора можно изучать схему защищённых методов).

use futures::FutureExt;
use futures::future::BoxFuture;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::future::Future;

#[derive(Debug, Clone, PartialEq)]
pub struct GwError {
    pub code: &'static str,
    pub message: String,
}

impl GwError {
    pub fn new(code: &'static str, message: &str) -> GwError {
        GwError { code, message: message.to_string() }
    }
}
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Principal {
    pub user_id: String,
    pub role: String,
}

#[derive(Debug, PartialEq)]
pub enum CheckError {
    Invalid,
    Expired,
}

pub trait TokenCheck: Send + Sync {
    fn validate(&self, token: &str) -> Result<(Principal, u64), CheckError>;
}

pub trait Clock: Send + Sync {
    fn now(&self) -> u64;
}

pub struct ManualClock(std::sync::atomic::AtomicU64);

impl ManualClock {
    pub fn new(start: u64) -> ManualClock {
        ManualClock(std::sync::atomic::AtomicU64::new(start))
    }

    pub fn set(&self, now: u64) {
        self.0.store(now, std::sync::atomic::Ordering::SeqCst);
    }
}

impl Clock for ManualClock {
    fn now(&self) -> u64 {
        self.0.load(std::sync::atomic::Ordering::SeqCst)
    }
}

pub struct Connection {
    pub principal: Option<Principal>,
}

pub struct Ctx {
    pub principal: Option<Principal>,
}

type Raw = Box<dyn Fn(Ctx, Value) -> BoxFuture<'static, Result<Value, GwError>> + Send + Sync>;

#[derive(Default)]
pub struct Gateway {
    table: HashMap<(String, String), Raw>,
}

fn decode<Req: DeserializeOwned>(data: Value) -> Result<Req, GwError> {
    serde_json::from_value(data).map_err(|_| GwError::new("invalid_argument", "данные запроса не подходят"))
}

fn encode<Resp: Serialize>(resp: Resp) -> Result<Value, GwError> {
    serde_json::to_value(resp).map_err(|_| GwError::new("internal", "ответ не сериализуется"))
}

impl Gateway {
    pub fn new() -> Gateway {
        Gateway::default()
    }

    pub fn route<Req, Resp, F, Fut>(&mut self, service: &str, method: &str, handler: F)
    where
        Req: DeserializeOwned + Send + 'static,
        Resp: Serialize + 'static,
        F: Fn(Ctx, Req) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Resp, GwError>> + Send + 'static,
    {
        let raw: Raw = Box::new(move |ctx, data| match decode::<Req>(data) {
            Err(error) => async move { Err(error) }.boxed(),
            Ok(req) => {
                let fut = handler(ctx, req);
                async move { encode(fut.await?) }.boxed()
            }
        });
        self.table.insert((service.to_string(), method.to_string()), raw);
    }

    pub fn route_authed<Req, Resp, F, Fut>(&mut self, service: &str, method: &str, handler: F)
    where
        Req: DeserializeOwned + Send + 'static,
        Resp: Serialize + 'static,
        F: Fn(Principal, Req) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Resp, GwError>> + Send + 'static,
    {
        let raw: Raw = Box::new(move |ctx, data| match decode::<Req>(data) {
            Err(error) => async move { Err(error) }.boxed(),
            Ok(req) => {
                let fut = handler(ctx.principal.unwrap_or_default(), req);
                async move { encode(fut.await?) }.boxed()
            }
        });
        self.table.insert((service.to_string(), method.to_string()), raw);
    }

    pub async fn call(&self, conn: &Connection, service: &str, method: &str, data: Value) -> Result<Value, GwError> {
        match self.table.get(&(service.to_string(), method.to_string())) {
            Some(handler) => handler(Ctx { principal: conn.principal.clone() }, data).await,
            None => Err(GwError::new("not_found", &format!("нет метода {service}.{method}"))),
        }
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Deserialize)]
    struct NewPost {
        text: String,
    }

    fn alice() -> Connection {
        Connection { principal: Some(Principal { user_id: "u-1".to_string(), role: "user".to_string() }) }
    }

    fn anonymous() -> Connection {
        Connection { principal: None }
    }

    fn gateway() -> (Gateway, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = calls.clone();
        let mut gateway = Gateway::new();
        gateway.route_authed("post", "create", move |who: Principal, req: NewPost| {
            seen.fetch_add(1, Ordering::SeqCst);
            async move { Ok(json!({"author": who.user_id, "text": req.text})) }
        });
        gateway.route("feed", "public", |ctx: Ctx, _: Value| async move { Ok(json!({"anonymous": ctx.principal.is_none()})) });
        (gateway, calls)
    }

    #[tokio::test]
    async fn authenticated_user_reaches_the_handler_as_a_principal() {
        let (gateway, calls) = gateway();
        let reply = gateway.call(&alice(), "post", "create", json!({"text": "привет"})).await;
        assert_eq!(reply, Ok(json!({"author": "u-1", "text": "привет"})), "Principal приходит без Option");
        assert_eq!(calls.load(Ordering::SeqCst), 1, "один вызов");
    }

    #[tokio::test]
    async fn anonymous_never_reaches_a_protected_handler() {
        let (gateway, calls) = gateway();
        let error = gateway.call(&anonymous(), "post", "create", json!({"text": "привет"})).await.unwrap_err();
        assert_eq!(error.code, "unauthenticated", "анониму нельзя");
        assert_eq!(calls.load(Ordering::SeqCst), 0, "обработчик не вызывался: подмена пустым пользователем это и есть дыра");
    }

    #[tokio::test]
    async fn auth_is_checked_before_the_data_is_parsed() {
        let (gateway, _) = gateway();
        let error = gateway.call(&anonymous(), "post", "create", json!({"wrong": 1})).await.unwrap_err();
        assert_eq!(error.code, "unauthenticated", "аноним получает отказ, а не подробности про схему защищённого метода");
        let error = gateway.call(&alice(), "post", "create", json!({"wrong": 1})).await.unwrap_err();
        assert_eq!(error.code, "invalid_argument", "а вошедшему честно говорим, что с данными");
    }

    #[tokio::test]
    async fn public_routes_work_for_everyone_and_see_the_option() {
        let (gateway, _) = gateway();
        assert_eq!(gateway.call(&anonymous(), "feed", "public", Value::Null).await, Ok(json!({"anonymous": true})), "аноним");
        assert_eq!(gateway.call(&alice(), "feed", "public", Value::Null).await, Ok(json!({"anonymous": false})), "вошедший");
    }
}
