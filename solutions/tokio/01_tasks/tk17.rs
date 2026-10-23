use std::time::Duration;

tokio::task_local! {
    static REQUEST_ID: u32;
}

fn log_line(message: &str) -> String {
    match REQUEST_ID.try_with(|id| *id) {
        Ok(id) => format!("[req {id}] {message}"),
        Err(_) => format!("[req -] {message}"),
    }
}

async fn handle(id: u32) -> String {
    REQUEST_ID
        .scope(id, async {
            tokio::time::sleep(Duration::from_millis(5)).await;
            log_line("готово")
        })
        .await
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
