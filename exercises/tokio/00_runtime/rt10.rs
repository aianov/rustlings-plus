// Подсчёт суммы от задачи-производителя. Код выглядит рабочим, на нескольких потоках
// он и работает, но на одном (current_thread, как в тестах по умолчанию) зависает насмерть.
// Тест защищён сторожем и сообщит о зависании, а не повиснет сам.
// TODO: Найди, что блокирует поток, и перепиши так, чтобы работало и там, и там.
use std::time::Duration;

async fn total_from_producer(count: u32) -> u32 {
    let (tx, rx) = std::sync::mpsc::channel();
    tokio::spawn(async move {
        for n in 1..=count {
            tokio::time::sleep(Duration::from_millis(1)).await;
            tx.send(n).unwrap();
        }
    });
    let mut total = 0;
    while let Ok(n) = rx.recv() {
        total += n;
    }
    total
}

#[tokio::main]
async fn main() {
    println!("сумма: {}", total_from_producer(5).await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use tokio::runtime::Builder;

    fn finishes_within<F>(seconds: u64, fut: F) -> Option<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .spawn(move || {
                let rt = Builder::new_current_thread().enable_all().build().unwrap();
                let _ = done_tx.send(rt.block_on(fut));
            })
            .expect("не удалось запустить поток");
        done_rx.recv_timeout(Duration::from_secs(seconds)).ok()
    }

    #[test]
    fn works_on_one_thread() {
        assert_eq!(
            finishes_within(3, total_from_producer(5)),
            Some(15),
            "на одном потоке подсчёт завис: ожидание в async-коде должно отдавать поток, а не держать его"
        );
    }

    #[test]
    fn works_on_many_threads() {
        let rt = Builder::new_multi_thread().worker_threads(2).enable_all().build().unwrap();
        assert_eq!(rt.block_on(total_from_producer(5)), 15, "и на пуле потоков сумма должна сойтись");
    }

    #[test]
    fn works_with_nothing_to_sum() {
        assert_eq!(finishes_within(3, total_from_producer(0)), Some(0), "пустой поток даёт 0");
    }
}
