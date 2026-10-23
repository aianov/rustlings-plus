// Токен живёт 15 минут, а WebSocket-соединение часами. Соединение авторизовано один раз при подключении и после этого
// живёт сколько угодно: токен давно протух, а проверок после подключения нет. Срок токена лежит в контексте соединения, и его надо сверять с часами на каждом вызове. Когда `now >= exp`,
// вызов отвечает ошибкой `token_expired`, соединение помечается закрытым, и все следующие вызовы в нём отвечают
// `closed` (клиент должен переподключиться с новым токеном). Анонимные соединения срока не имеют.
// Цель: `Gateway::call` проверяет срок до диспетчеризации, для любого метода, публичного тоже.

use futures::FutureExt;
use futures::future::BoxFuture;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

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
    pub exp: Option<u64>,
    pub closed: bool,
}

pub struct Ctx {
    pub principal: Option<Principal>,
}

type Raw = Box<dyn Fn(Ctx, Value) -> BoxFuture<'static, Result<Value, GwError>> + Send + Sync>;

#[derive(Default)]
pub struct Gateway {
    table: HashMap<(String, String), Raw>,
    clock: Option<Arc<dyn Clock>>,
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
        let raw: Raw = Box::new(move |ctx, data| {
            let Some(principal) = ctx.principal else {
                return async { Err(GwError::new("unauthenticated", "нужно войти")) }.boxed();
            };
            match decode::<Req>(data) {
                Err(error) => async move { Err(error) }.boxed(),
                Ok(req) => {
                    let fut = handler(principal, req);
                    async move { encode(fut.await?) }.boxed()
                }
            }
        });
        self.table.insert((service.to_string(), method.to_string()), raw);
    }
}
impl Gateway {
    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Gateway {
        self.clock = Some(clock);
        self
    }

    pub async fn call(&self, conn: &mut Connection, service: &str, method: &str, data: Value) -> Result<Value, GwError> {
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
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn setup() -> (Gateway, Arc<ManualClock>, Arc<AtomicUsize>) {
        let clock = Arc::new(ManualClock::new(900));
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = calls.clone();
        let mut gateway = Gateway::new().with_clock(clock.clone());
        gateway.route_authed("post", "create", move |who: Principal, _: Value| {
            seen.fetch_add(1, Ordering::SeqCst);
            async move { Ok(json!(who.user_id)) }
        });
        gateway.route("feed", "public", |_: Ctx, _: Value| async { Ok(json!("лента")) });
        (gateway, clock, calls)
    }

    fn alice(exp: u64) -> Connection {
        Connection { principal: Some(Principal { user_id: "u-1".to_string(), role: "user".to_string() }), exp: Some(exp), closed: false }
    }

    #[tokio::test]
    async fn connection_works_until_the_token_expires() {
        let (gateway, clock, calls) = setup();
        let mut conn = alice(1_000);
        assert!(gateway.call(&mut conn, "post", "create", Value::Null).await.is_ok(), "сразу после подключения");
        clock.set(999);
        assert!(gateway.call(&mut conn, "post", "create", Value::Null).await.is_ok(), "за секунду до срока ещё можно");
        assert_eq!(calls.load(Ordering::SeqCst), 2, "два вызова дошли");
    }

    #[tokio::test]
    async fn at_the_deadline_the_call_is_refused_and_the_connection_closed() {
        let (gateway, clock, calls) = setup();
        let mut conn = alice(1_000);
        clock.set(1_000);
        let error = gateway.call(&mut conn, "post", "create", Value::Null).await.unwrap_err();
        assert_eq!(error.code, "token_expired", "срок вышел: токен протух, соединение авторизовано уже не по нему");
        assert!(conn.closed, "соединение помечено закрытым");
        assert_eq!(calls.load(Ordering::SeqCst), 0, "обработчик не вызывался");
    }

    #[tokio::test]
    async fn public_methods_do_not_outlive_the_token_either() {
        let (gateway, clock, _) = setup();
        let mut conn = alice(1_000);
        clock.set(2_000);
        let error = gateway.call(&mut conn, "feed", "public", Value::Null).await.unwrap_err();
        assert_eq!(error.code, "token_expired", "для авторизованного соединения срок проверяется перед любым методом");
    }

    #[tokio::test]
    async fn closed_connection_stays_closed_even_if_the_clock_goes_back() {
        let (gateway, clock, _) = setup();
        let mut conn = alice(1_000);
        clock.set(1_500);
        gateway.call(&mut conn, "feed", "public", Value::Null).await.unwrap_err();
        clock.set(100);
        let error = gateway.call(&mut conn, "feed", "public", Value::Null).await.unwrap_err();
        assert_eq!(error.code, "closed", "закрытое остаётся закрытым: ответ `closed` без повторной проверки срока");
    }

    #[tokio::test]
    async fn anonymous_connections_have_no_deadline() {
        let (gateway, clock, _) = setup();
        let mut conn = Connection { principal: None, exp: None, closed: false };
        clock.set(10_000_000);
        assert_eq!(gateway.call(&mut conn, "feed", "public", Value::Null).await, Ok(json!("лента")), "аноним живёт сколько угодно");
        assert!(!conn.closed, "и не закрывается");
    }
}
