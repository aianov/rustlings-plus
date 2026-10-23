
use futures::FutureExt;
use futures::future::BoxFuture;
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

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    #[tokio::test]
    async fn handlers_with_different_future_types_live_in_one_table() {
        let mut router = Router::new();
        router.add("user", "now", |_| async { Ok(json!("now")) });
        router.add("user", "twice", |data| async move {
            tokio::task::yield_now().await;
            tokio::task::yield_now().await;
            Ok(json!([data.clone(), data]))
        });
        assert_eq!(router.call("user", "now", Value::Null).await, Ok(json!("now")), "первый обработчик");
        assert_eq!(router.call("user", "twice", json!(1)).await, Ok(json!([1, 1])), "второй обработчик, другой тип future");
    }

    #[tokio::test]
    async fn handlers_can_capture_shared_state() {
        let counter = Arc::new(AtomicUsize::new(0));
        let mut router = Router::new();
        let seen = counter.clone();
        router.add("stats", "hit", move |_| {
            let seen = seen.clone();
            async move { Ok(json!(seen.fetch_add(1, Ordering::SeqCst) + 1)) }
        });
        for expected in 1..=3 {
            assert_eq!(router.call("stats", "hit", Value::Null).await, Ok(json!(expected)), "вызов {expected}");
        }
    }

    #[tokio::test]
    async fn unknown_route_is_not_found_and_errors_pass_through() {
        let mut router = Router::new();
        router.add("user", "boom", |_| async { Err(GwError::new("internal", "упало")) });
        assert_eq!(router.call("user", "boom", Value::Null).await.unwrap_err().code, "internal", "ошибка обработчика");
        assert_eq!(router.call("user", "nope", Value::Null).await.unwrap_err().code, "not_found", "нет такого метода");
        assert_eq!(router.call("ghost", "boom", Value::Null).await.unwrap_err().code, "not_found", "нет такого сервиса");
    }

    #[tokio::test(start_paused = true)]
    async fn slow_handlers_do_not_serialize_each_other() {
        let mut router = Router::new();
        router.add("a", "slow", |_| async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            Ok(json!(1))
        });
        let router = Arc::new(router);
        let started = tokio::time::Instant::now();
        let calls: Vec<_> = (0..5)
            .map(|_| {
                let router = router.clone();
                tokio::spawn(async move { router.call("a", "slow", Value::Null).await })
            })
            .collect();
        for call in calls {
            assert!(call.await.unwrap().is_ok(), "ответ пришёл");
        }
        assert_eq!(started.elapsed(), Duration::from_millis(100), "пять вызовов вместе заняли 100 мс, а не 500: `call` берёт `&self` и ничего не блокирует");
    }
}
