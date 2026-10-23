
use futures::future::BoxFuture;
use serde_json::{Value, json};
use std::time::Duration;

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

pub trait Backend: Send + Sync {
    fn user(&self, id: u64) -> BoxFuture<'_, Result<Value, GwError>>;
    fn posts(&self, id: u64) -> BoxFuture<'_, Result<Value, GwError>>;
    fn stats(&self, id: u64) -> BoxFuture<'_, Result<Value, GwError>>;
}

pub const OPTIONAL_BUDGET: Duration = Duration::from_millis(150);

async fn optional(fut: BoxFuture<'_, Result<Value, GwError>>) -> Option<Value> {
    tokio::time::timeout(OPTIONAL_BUDGET, fut).await.ok().and_then(Result::ok)
}

pub async fn profile(backend: &dyn Backend, id: u64) -> Result<Value, GwError> {
    let (user, posts, stats) = tokio::join!(backend.user(id), optional(backend.posts(id)), optional(backend.stats(id)));
    let user = user?;
    let mut partial = Vec::new();
    if posts.is_none() {
        partial.push("posts");
    }
    if stats.is_none() {
        partial.push("stats");
    }
    Ok(json!({"user": user, "posts": posts, "stats": stats, "partial": partial}))
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::FutureExt;
    use tokio::time::Instant;

    #[derive(Clone, Copy)]
    enum Mode {
        Ok,
        Fail,
        Hang,
    }

    struct Fake {
        user: Mode,
        posts: Mode,
        stats: Mode,
    }

    async fn run(mode: Mode, value: Value) -> Result<Value, GwError> {
        match mode {
            Mode::Ok => {
                tokio::time::sleep(Duration::from_millis(100)).await;
                Ok(value)
            }
            Mode::Fail => {
                tokio::time::sleep(Duration::from_millis(20)).await;
                Err(GwError::new("unavailable", "сервис недоступен"))
            }
            Mode::Hang => {
                tokio::time::sleep(Duration::from_secs(3600)).await;
                Ok(value)
            }
        }
    }

    impl Backend for Fake {
        fn user(&self, id: u64) -> BoxFuture<'_, Result<Value, GwError>> {
            run(self.user, json!({"id": id, "name": "Алиса"})).boxed()
        }
        fn posts(&self, _: u64) -> BoxFuture<'_, Result<Value, GwError>> {
            run(self.posts, json!([1, 2])).boxed()
        }
        fn stats(&self, _: u64) -> BoxFuture<'_, Result<Value, GwError>> {
            run(self.stats, json!({"friends": 3})).boxed()
        }
    }

    fn fake(user: Mode, posts: Mode, stats: Mode) -> Fake {
        Fake { user, posts, stats }
    }

    #[tokio::test(start_paused = true)]
    async fn three_calls_take_as_long_as_the_slowest() {
        let started = Instant::now();
        let reply = profile(&fake(Mode::Ok, Mode::Ok, Mode::Ok), 7).await.unwrap();
        assert_eq!(started.elapsed(), Duration::from_millis(100), "три запроса по 100 мс идут одновременно, а не подряд");
        assert_eq!(reply["user"]["name"], "Алиса", "пользователь на месте");
        assert_eq!(reply["posts"], json!([1, 2]), "посты на месте");
        assert_eq!(reply["stats"]["friends"], 3, "счётчики на месте");
        assert_eq!(reply["partial"], json!([]), "ничего не потеряно");
    }

    #[tokio::test(start_paused = true)]
    async fn failed_optional_part_makes_the_reply_partial_not_failed() {
        let reply = profile(&fake(Mode::Ok, Mode::Ok, Mode::Fail), 7).await.expect("профиль без счётчиков лучше ошибки");
        assert_eq!(reply["stats"], Value::Null, "счётчиков нет");
        assert_eq!(reply["partial"], json!(["stats"]), "список недостающих частей");
        assert_eq!(reply["user"]["id"], 7, "пользователь получен");
        let reply = profile(&fake(Mode::Ok, Mode::Fail, Mode::Fail), 7).await.unwrap();
        assert_eq!(reply["partial"], json!(["posts", "stats"]), "обе необязательные части, в порядке полей");
    }

    #[tokio::test(start_paused = true)]
    async fn a_hanging_optional_part_costs_only_its_budget() {
        let started = Instant::now();
        let reply = profile(&fake(Mode::Ok, Mode::Hang, Mode::Ok), 7).await.unwrap();
        assert_eq!(started.elapsed(), OPTIONAL_BUDGET, "зависшие посты ждали ровно отведённые 150 мс, а не вечность");
        assert_eq!(reply["partial"], json!(["posts"]), "посты отмечены недостающими");
        assert_eq!(reply["stats"]["friends"], 3, "остальное цело");
    }

    #[tokio::test(start_paused = true)]
    async fn the_required_user_fails_the_whole_reply() {
        let error = profile(&fake(Mode::Fail, Mode::Ok, Mode::Ok), 7).await.unwrap_err();
        assert_eq!(error.code, "unavailable", "без пользователя профиля нет");
    }

    #[tokio::test(start_paused = true)]
    async fn slow_user_is_waited_for_beyond_the_optional_budget() {
        struct SlowUser;
        impl Backend for SlowUser {
            fn user(&self, id: u64) -> BoxFuture<'_, Result<Value, GwError>> {
                async move {
                    tokio::time::sleep(Duration::from_millis(400)).await;
                    Ok(json!({"id": id}))
                }
                .boxed()
            }
            fn posts(&self, _: u64) -> BoxFuture<'_, Result<Value, GwError>> {
                run(Mode::Ok, json!([])).boxed()
            }
            fn stats(&self, _: u64) -> BoxFuture<'_, Result<Value, GwError>> {
                run(Mode::Ok, json!({})).boxed()
            }
        }
        let reply = profile(&SlowUser, 1).await.unwrap();
        assert_eq!(reply["user"]["id"], 1, "обязательную часть бюджет в 150 мс не режет");
        assert_eq!(reply["partial"], json!([]), "необязательные успели");
    }
}
