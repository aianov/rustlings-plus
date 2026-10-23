// В сервисе есть критичная фоновая задача: индексатор. Он упал с паникой, а сервис продолжает
// работать как ни в чём не бывало: принимает запросы, отдаёт устаревшие данные, и никто не в
// курсе. `run_all` ждёт все задачи и вежливо отчитывается «всё хорошо». Паника в задаче это
// ошибка, которую `JoinSet` отдаёт в `join_next`, и сейчас её выбрасывают.
// TODO: Почини `run_all`: паника критичной задачи останавливает остальные и возвращает
// ошибку с её именем; паники некритичных считаются и возвращаются в `Ok`.

use futures::future::BoxFuture;
use std::collections::HashMap;
use tokio::task::JoinSet;

pub struct Task {
    pub name: &'static str,
    pub critical: bool,
    pub future: BoxFuture<'static, ()>,
}

#[derive(Debug, PartialEq)]
pub enum RunError {
    CriticalPanicked(&'static str),
}

pub async fn run_all(tasks: Vec<Task>) -> Result<usize, RunError> {
    let mut set = JoinSet::new();
    let mut meta = HashMap::new();
    for task in tasks {
        let handle = set.spawn(task.future);
        meta.insert(handle.id(), (task.name, task.critical));
    }
    while set.join_next_with_id().await.is_some() {}
    Ok(0)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;
    use tokio::time::sleep;

    fn task(name: &'static str, critical: bool, future: BoxFuture<'static, ()>) -> Task {
        Task { name, critical, future }
    }

    #[tokio::test(start_paused = true)]
    async fn clean_exit_is_ok() {
        let result = run_all(vec![task("a", true, Box::pin(async {})), task("b", false, Box::pin(async {}))]).await;
        assert_eq!(result, Ok(0), "никто не падал: Ok(0)");
    }

    #[tokio::test(start_paused = true)]
    async fn critical_panic_is_reported_and_stops_the_others() {
        let still_running = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&still_running);
        let long = task("sync", false, Box::pin(async move {
            sleep(Duration::from_secs(60)).await;
            flag.store(true, Ordering::SeqCst);
        }));
        let broken = task("indexer", true, Box::pin(async {
            sleep(Duration::from_millis(10)).await;
            panic!("индекс повреждён");
        }));
        let result = tokio::time::timeout(Duration::from_secs(120), run_all(vec![long, broken]))
            .await
            .expect("run_all завис");
        assert_eq!(result, Err(RunError::CriticalPanicked("indexer")), "паника критичной задачи должна дойти до вызывающего");
        sleep(Duration::from_secs(100)).await;
        assert!(!still_running.load(Ordering::SeqCst), "после критичного отказа остальные задачи должны быть остановлены");
    }

    #[tokio::test(start_paused = true)]
    async fn non_critical_panics_are_counted() {
        let result = run_all(vec![
            task("mailer", false, Box::pin(async { panic!("smtp") })),
            task("cleanup", false, Box::pin(async { panic!("disk") })),
            task("ok", true, Box::pin(async {})),
        ])
        .await;
        assert_eq!(result, Ok(2), "две некритичные паники: Ok(2), а процесс жив");
    }
}
