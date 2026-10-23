// Два тихих дефекта в роутере. Во-первых, два разработчика добавили `user.get`, и второй обработчик молча затёр
// первый: всё собралось, а поведение поменялось. Дубликат маршрута должен ловиться при старте, и в ошибке названа
// каждая задвоенная пара. Во-вторых, неверные данные клиента сейчас уходят как `internal` (то есть 500), и
// мониторинг будит дежурного из-за чужих опечаток. Цель: `RouterBuilder::build` возвращает ошибку со всеми
// дубликатами (по одному разу каждая пара, в порядке появления), а плохие данные дают `invalid_argument`.

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

pub fn ok_reply(id: &Value, data: Value) -> Value {
    json!({"id": id, "ok": true, "data": data})
}

pub fn err_reply(id: &Value, error: &GwError) -> Value {
    json!({"id": id, "ok": false, "error": {"code": error.code, "message": error.message}})
}

pub type Handler = Box<dyn Fn(Value) -> BoxFuture<'static, Result<Value, GwError>> + Send + Sync>;

pub struct Router {
    table: HashMap<(String, String), Handler>,
}

impl Router {
    pub async fn call(&self, service: &str, method: &str, data: Value) -> Result<Value, GwError> {
        match self.table.get(&(service.to_string(), method.to_string())) {
            Some(handler) => handler(data).await,
            None => Err(GwError::new("not_found", &format!("нет метода {service}.{method}"))),
        }
    }
}

#[derive(Default)]
pub struct RouterBuilder {
    table: HashMap<(String, String), Handler>,
}

impl RouterBuilder {
    pub fn new() -> RouterBuilder {
        RouterBuilder::default()
    }

    pub fn add<F, Fut>(mut self, service: &str, method: &str, handler: F) -> RouterBuilder
    where
        F: Fn(Value) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Value, GwError>> + Send + 'static,
    {
        self.table.insert((service.to_string(), method.to_string()), Box::new(move |data| handler(data).boxed()));
        self
    }

    pub fn route<Req, Resp, F, Fut>(self, service: &str, method: &str, handler: F) -> RouterBuilder
    where
        Req: DeserializeOwned + Send + 'static,
        Resp: Serialize + 'static,
        F: Fn(Req) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Resp, GwError>> + Send + 'static,
    {
        self.add(service, method, move |data| match serde_json::from_value::<Req>(data) {
            Err(_) => async { Err(GwError::new("internal", "ошибка разбора")) }.left_future(),
            Ok(req) => {
                let fut = handler(req);
                async move {
                    let resp = fut.await?;
                    serde_json::to_value(resp).map_err(|_| GwError::new("internal", "ответ не сериализуется"))
                }
                .right_future()
            }
        })
    }

    pub fn build(self) -> Result<Router, Vec<String>> {
        Ok(Router { table: self.table })
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct Req {
        id: u64,
    }

    async fn ok(_: Value) -> Result<Value, GwError> {
        Ok(json!(1))
    }

    #[test]
    fn duplicate_routes_fail_the_startup_and_are_all_named() {
        let result = RouterBuilder::new()
            .add("user", "get", ok)
            .add("user", "list", ok)
            .add("user", "get", ok)
            .add("post", "create", ok)
            .add("post", "create", ok)
            .add("user", "get", ok)
            .build();
        assert_eq!(result.err(), Some(vec!["user.get".to_string(), "post.create".to_string()]), "каждая задвоенная пара названа один раз, в порядке появления");
    }

    #[test]
    fn same_method_name_in_different_services_is_fine() {
        let router = RouterBuilder::new().add("user", "get", ok).add("post", "get", ok).build();
        assert!(router.is_ok(), "user.get и post.get это разные маршруты");
    }

    #[tokio::test]
    async fn the_first_handler_is_not_replaced_before_build_even_runs() {
        let built = RouterBuilder::new().add("a", "b", |_| async { Ok(json!("первый")) }).add("a", "b", |_| async { Ok(json!("второй")) }).build();
        assert!(built.is_err(), "дубликат это ошибка старта, а не победа последнего");
    }

    #[tokio::test]
    async fn bad_client_data_is_invalid_argument() {
        let router = RouterBuilder::new().route("user", "get", |req: Req| async move { Ok(json!(req.id)) }).build().unwrap();
        assert_eq!(router.call("user", "get", json!({"id": 5})).await, Ok(json!(5)), "верные данные");
        for data in [json!({"id": "x"}), json!(null), json!({})] {
            let error = router.call("user", "get", data.clone()).await.unwrap_err();
            assert_eq!(error.code, "invalid_argument", "{data}: это ошибка клиента, 500 тут ложь");
        }
    }

    #[tokio::test]
    async fn unknown_route_still_works() {
        let router = RouterBuilder::new().add("a", "b", ok).build().unwrap();
        assert_eq!(router.call("a", "c", Value::Null).await.unwrap_err().code, "not_found", "нет такого метода");
    }
}
