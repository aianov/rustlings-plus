// Таблица команд: имя команды и асинхронный обработчик. Обработчики бывают разные: обычные
// `async fn`, замыкания, захватившие общее состояние. Таблица хранит их одним типом `Handler`,
// а вызывается из многих задач сразу.
// TODO: Допиши `register` (принять любой подходящий обработчик и положить в таблицу; обработчик
// с тем же именем заменяется) и `dispatch` (неизвестное имя даёт `Resp::NotFound`). Ограничения на
// типы в `register` придётся вывести самому, подсказывать будет компилятор и тесты.
use futures::FutureExt;
use futures::future::BoxFuture;
use std::collections::HashMap;
use std::future::Future;

#[derive(Debug, Clone, PartialEq)]
struct Req {
    args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
enum Resp {
    Ok(String),
    NotFound,
}

type Handler = Box<dyn Fn(Req) -> BoxFuture<'static, Resp> + Send + Sync>;

struct Router {
    handlers: HashMap<String, Handler>,
}

impl Router {
    fn new() -> Router {
        Router { handlers: HashMap::new() }
    }
}

impl Router {
    fn register<F, Fut>(&mut self, name: &str, handler: F) {
        todo!()
    }

    async fn dispatch(&self, name: &str, req: Req) -> Resp {
        todo!()
    }
}

async fn shout(req: Req) -> Resp {
    Resp::Ok(req.args.join(" ").to_uppercase())
}

#[tokio::main]
async fn main() {
    let mut router = Router::new();
    router.register("shout", shout);
    println!("{:?}", router.dispatch("shout", Req { args: vec!["привет".into()] }).await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;
    use tokio::time::{Instant, sleep};

    fn req(args: &[&str]) -> Req {
        Req { args: args.iter().map(|arg| arg.to_string()).collect() }
    }

    async fn echo(req: Req) -> Resp {
        Resp::Ok(req.args.join(" "))
    }

    async fn count(req: Req) -> Resp {
        Resp::Ok(req.args.len().to_string())
    }

    #[tokio::test]
    async fn plain_async_fns_are_handlers() {
        let mut router = Router::new();
        router.register("echo", echo);
        router.register("count", count);
        assert_eq!(router.dispatch("echo", req(&["a", "b"])).await, Resp::Ok("a b".into()), "echo склеивает аргументы");
        assert_eq!(router.dispatch("count", req(&["a", "b", "c"])).await, Resp::Ok("3".into()), "count считает аргументы");
    }

    #[tokio::test]
    async fn unknown_command_is_not_found_and_registering_again_replaces() {
        let mut router = Router::new();
        assert_eq!(router.dispatch("echo", req(&[])).await, Resp::NotFound, "в пустой таблице всё NotFound");
        router.register("echo", echo);
        router.register("echo", count);
        assert_eq!(router.dispatch("echo", req(&["x", "y"])).await, Resp::Ok("2".into()), "повторная регистрация заменяет обработчик");
    }

    #[tokio::test]
    async fn closures_may_capture_shared_state() {
        let hits = Arc::new(AtomicUsize::new(0));
        let mut router = Router::new();
        let seen = Arc::clone(&hits);
        router.register("hit", move |_req| {
            let seen = Arc::clone(&seen);
            async move { Resp::Ok(seen.fetch_add(1, Ordering::SeqCst).to_string()) }
        });
        assert_eq!(router.dispatch("hit", req(&[])).await, Resp::Ok("0".into()), "первый вызов видит ноль");
        assert_eq!(router.dispatch("hit", req(&[])).await, Resp::Ok("1".into()), "состояние общее для вызовов");
        assert_eq!(hits.load(Ordering::SeqCst), 2, "счётчик снаружи тоже изменился");
    }

    #[tokio::test(start_paused = true)]
    async fn table_is_shared_between_tasks() {
        let mut router = Router::new();
        router.register("echo", echo);
        router.register("slow", |_req| async {
            sleep(Duration::from_millis(50)).await;
            Resp::Ok("done".to_string())
        });
        let router = Arc::new(router);
        let start = Instant::now();
        let tasks: Vec<_> = (0..4)
            .map(|n| {
                let router = Arc::clone(&router);
                tokio::spawn(async move {
                    let slow = router.dispatch("slow", req(&[])).await;
                    let echoed = router.dispatch("echo", req(&[&n.to_string()])).await;
                    (slow, echoed)
                })
            })
            .collect();
        for (n, task) in tasks.into_iter().enumerate() {
            let (slow, echoed) = task.await.unwrap();
            assert_eq!(slow, Resp::Ok("done".into()), "медленный обработчик отвечает из любой задачи");
            assert_eq!(echoed, Resp::Ok(n.to_string()), "аргументы не перемешиваются между задачами");
        }
        assert_eq!(start.elapsed(), Duration::from_millis(50), "четыре задачи шли вместе, а не по очереди");
    }
}
