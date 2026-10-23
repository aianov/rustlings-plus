// Остановка с дедлайном. Токен отмены все задачи получили, вежливые доделывают текущую работу и
// выходят. Но бывает и упрямая задача, которая токена не слушает (чужой код, забытая проверка), и тогда
// остановка висела бы вечно. Нужен срок: сколько дадим, столько ждём, потом принудительно.
// TODO: Допиши `stop_all`: отменить токен, дождаться задач из набора не дольше `deadline`, остальные
// остановить силой и вернуть счёт. Когда все вышли раньше срока, дожидаться срока нельзя.
use futures::future::select_all;
use std::sync::Arc;
use tokio::sync::watch;

#[derive(Clone)]
struct Token {
    own: Arc<watch::Sender<bool>>,
    parents: Vec<Arc<watch::Sender<bool>>>,
}

#[allow(dead_code)]
impl Token {
    fn new() -> Token {
        Token { own: Arc::new(watch::channel(false).0), parents: Vec::new() }
    }

    fn child(&self) -> Token {
        let mut parents = self.parents.clone();
        parents.push(self.own.clone());
        Token { own: Arc::new(watch::channel(false).0), parents }
    }

    fn cancel(&self) {
        self.own.send_replace(true);
    }

    fn is_cancelled(&self) -> bool {
        *self.own.borrow() || self.parents.iter().any(|flag| *flag.borrow())
    }

    async fn cancelled(&self) {
        let waits: Vec<_> = std::iter::once(&self.own)
            .chain(self.parents.iter())
            .map(|flag| {
                let mut rx = flag.subscribe();
                Box::pin(async move {
                    let _ = rx.wait_for(|cancelled| *cancelled).await;
                })
            })
            .collect();
        select_all(waits).await;
    }
}

use std::time::Duration;
use tokio::task::JoinSet;
use tokio::time::{Instant, timeout_at};

#[derive(Debug, PartialEq)]
struct Stopped {
    finished: usize,
    aborted: usize,
}

async fn stop_all(token: &Token, tasks: JoinSet<()>, deadline: Duration) -> Stopped {
    todo!()
}

#[tokio::main]
async fn main() {
    let token = Token::new();
    let mut tasks = JoinSet::new();
    let polite = token.clone();
    tasks.spawn(async move { polite.cancelled().await });
    println!("{:?}", stop_all(&token, tasks, Duration::from_millis(100)).await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tokio::time::sleep;

    struct Note(Arc<Mutex<Vec<&'static str>>>);

    impl Drop for Note {
        fn drop(&mut self) {
            self.0.lock().unwrap().push("упрямая задача уничтожена");
        }
    }

    async fn polite(token: Token, cleanup_ms: u64) {
        token.cancelled().await;
        sleep(Duration::from_millis(cleanup_ms)).await;
    }

    async fn stubborn(log: Arc<Mutex<Vec<&'static str>>>) {
        let _note = Note(log);
        loop {
            sleep(Duration::from_millis(10)).await;
        }
    }

    #[tokio::test(start_paused = true)]
    async fn polite_tasks_finish_stubborn_ones_are_aborted_at_the_deadline() {
        let token = Token::new();
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut tasks = JoinSet::new();
        tasks.spawn(polite(token.clone(), 50));
        tasks.spawn(polite(token.clone(), 80));
        tasks.spawn(stubborn(log.clone()));
        let begin = Instant::now();
        let stopped = stop_all(&token, tasks, Duration::from_millis(200)).await;
        assert_eq!(stopped, Stopped { finished: 2, aborted: 1 }, "две вежливые вышли сами, упрямую пришлось остановить силой");
        assert_eq!(begin.elapsed(), Duration::from_millis(200), "упрямая задача держит остановку до самого дедлайна");
        assert_eq!(
            *log.lock().unwrap(),
            vec!["упрямая задача уничтожена"],
            "к моменту возврата остановленная задача должна быть уже уничтожена, а не просто помечена"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn polite_tasks_do_not_wait_for_the_deadline() {
        let token = Token::new();
        let mut tasks = JoinSet::new();
        tasks.spawn(polite(token.clone(), 50));
        tasks.spawn(polite(token.clone(), 80));
        let begin = Instant::now();
        let stopped = stop_all(&token, tasks, Duration::from_secs(10)).await;
        assert_eq!(stopped, Stopped { finished: 2, aborted: 0 }, "обе вышли сами");
        assert_eq!(begin.elapsed(), Duration::from_millis(80), "остановка ждёт самую медленную вежливую, а не срок");
    }

    #[tokio::test(start_paused = true)]
    async fn nothing_to_stop_and_all_stubborn() {
        let token = Token::new();
        let begin = Instant::now();
        let stopped = stop_all(&token, JoinSet::new(), Duration::from_secs(1)).await;
        assert_eq!(stopped, Stopped { finished: 0, aborted: 0 }, "задач нет");
        assert_eq!(begin.elapsed(), Duration::ZERO, "пустой набор не ждёт");

        let log = Arc::new(Mutex::new(Vec::new()));
        let mut tasks = JoinSet::new();
        for _ in 0..3 {
            tasks.spawn(stubborn(log.clone()));
        }
        let begin = Instant::now();
        let stopped = stop_all(&token, tasks, Duration::from_millis(200)).await;
        assert_eq!(stopped, Stopped { finished: 0, aborted: 3 }, "ни одна не послушалась");
        assert_eq!(begin.elapsed(), Duration::from_millis(200), "ровно дедлайн");
        assert_eq!(log.lock().unwrap().len(), 3, "все три остановлены до возврата");
    }
}
