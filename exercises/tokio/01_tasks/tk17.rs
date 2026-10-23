// У каждого запроса есть id, и он попадает в каждую строку журнала. Автор положил его
// в thread_local, как делают в синхронном коде. В async это ломается: поток не принадлежит
// задаче. Пока одна задача ждёт на .await, на том же потоке работают другие и перезаписывают
// значение, а на multi_thread задача ещё и переезжает на другой поток.
// Формат строки: "[req <id>] сообщение", а вне запроса вместо id пишется "-".
// TODO: Сделай так, чтобы у каждого запроса в журнале был свой id при любом чередовании задач.
use std::time::Duration;

use std::cell::Cell;

thread_local! {
    static REQUEST_ID: Cell<u32> = const { Cell::new(0) };
}

fn log_line(message: &str) -> String {
    format!("[req {}] {message}", REQUEST_ID.get())
}

async fn handle(id: u32) -> String {
    REQUEST_ID.set(id);
    tokio::time::sleep(Duration::from_millis(5)).await;
    log_line("готово")
}

#[tokio::main]
async fn main() {
    println!("{}", handle(1).await);
    println!("{}", log_line("вне запроса"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn two_requests_in_one_task_do_not_mix() {
        let (first, second) = tokio::join!(handle(1), handle(2));
        assert_eq!(first, "[req 1] готово", "у первого запроса свой id");
        assert_eq!(second, "[req 2] готово", "второй запрос перезаписал id первого: значение должно жить в задаче, а не в потоке");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 3)]
    async fn many_spawned_requests() {
        let handles: Vec<_> = (1..=20).map(|id| tokio::spawn(handle(id))).collect();
        for (index, handle) in handles.into_iter().enumerate() {
            let id = index + 1;
            assert_eq!(handle.await.unwrap(), format!("[req {id}] готово"), "каждая задача видит только свой id");
        }
    }

    #[test]
    fn outside_of_request() {
        assert_eq!(log_line("привет"), "[req -] привет", "вне запроса id нет, а не «0» или чужой");
    }
}
