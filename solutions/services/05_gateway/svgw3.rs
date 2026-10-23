
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

#[derive(Default)]
pub struct Router {
    table: HashMap<(String, String), Handler>,
}

impl Router {
    pub fn new() -> Router {
        Router::default()
    }

    pub fn add<F, Fut>(&mut self, service: &str, method: &str, handler: F)
    where
        F: Fn(Value) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Value, GwError>> + Send + 'static,
    {
        self.table.insert((service.to_string(), method.to_string()), Box::new(move |data| handler(data).boxed()));
    }

    pub async fn call(&self, service: &str, method: &str, data: Value) -> Result<Value, GwError> {
        match self.table.get(&(service.to_string(), method.to_string())) {
            Some(handler) => handler(data).await,
            None => Err(GwError::new("not_found", &format!("нет метода {service}.{method}"))),
        }
    }
}

impl Router {
    pub fn route<Req, Resp, F, Fut>(&mut self, service: &str, method: &str, handler: F)
    where
        Req: DeserializeOwned + Send + 'static,
        Resp: Serialize + 'static,
        F: Fn(Req) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Resp, GwError>> + Send + 'static,
    {
        self.add(service, method, move |data| match serde_json::from_value::<Req>(data) {
            Err(_) => async { Err(GwError::new("invalid_argument", "данные запроса не подходят")) }.left_future(),
            Ok(req) => {
                let fut = handler(req);
                async move {
                    let resp = fut.await?;
                    serde_json::to_value(resp).map_err(|_| GwError::new("internal", "ответ не сериализуется"))
                }
                .right_future()
            }
        });
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct GetUser {
        id: u64,
    }

    #[derive(Serialize)]
    struct UserDto {
        id: u64,
        name: String,
    }

    #[derive(Deserialize)]
    struct Login {
        #[allow(dead_code)]
        password: String,
    }

    fn router() -> Router {
        let mut router = Router::new();
        router.route("user", "get", |req: GetUser| async move { Ok(UserDto { id: req.id, name: format!("user-{}", req.id) }) });
        router.route("user", "login", |_: Login| async { Ok(json!({"ok": true})) });
        router.route("user", "fail", |_: GetUser| async { Err::<UserDto, _>(GwError::new("not_found", "нет")) });
        router
    }

    #[tokio::test]
    async fn typed_request_in_typed_response_out() {
        let reply = router().call("user", "get", json!({"id": 7})).await;
        assert_eq!(reply, Ok(json!({"id": 7, "name": "user-7"})), "обработчик получил GetUser и вернул UserDto");
    }

    #[tokio::test]
    async fn bad_data_is_invalid_argument_and_never_reaches_the_handler() {
        for data in [json!({"id": "семь"}), json!({}), json!(null), json!("текст")] {
            let error = router().call("user", "get", data.clone()).await.unwrap_err();
            assert_eq!(error.code, "invalid_argument", "данные {data} не годятся: это ошибка клиента, а не 500");
        }
    }

    #[tokio::test]
    async fn sent_values_are_not_echoed_back() {
        let error = router().call("user", "login", json!({"password": 12345678})).await.unwrap_err();
        assert!(!error.message.contains("12345678"), "сообщение об ошибке не должно цитировать присланное значение: {}", error.message);
    }

    #[tokio::test]
    async fn handler_errors_pass_through_untouched() {
        let error = router().call("user", "fail", json!({"id": 1})).await.unwrap_err();
        assert_eq!((error.code, error.message.as_str()), ("not_found", "нет"), "ошибка обработчика не заменяется");
    }

    #[tokio::test]
    async fn unserializable_response_is_internal() {
        struct Broken;
        impl Serialize for Broken {
            fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
                Err(serde::ser::Error::custom("не получилось"))
            }
        }
        let mut router = Router::new();
        router.route("x", "y", |_: GetUser| async { Ok(Broken) });
        let error = router.call("x", "y", json!({"id": 1})).await.unwrap_err();
        assert_eq!(error.code, "internal", "ошибка сериализации ответа это наша вина");
    }
}
