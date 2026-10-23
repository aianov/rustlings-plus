// Vec<JoinHandle> отдаёт результаты в порядке запуска: первый хэндл ждём, даже если остальные
// давно готовы. JoinSet отдаёт их в порядке готовности через join_next. Для сбора ответов
// «кто первый, того и тапки» это именно то, что нужно.
// TODO: Допиши in_finish_order на JoinSet: порядок такой, в каком задачи закончились.
use std::time::Duration;
use tokio::task::JoinSet;

async fn delayed(ms: u64) -> u64 {
    tokio::time::sleep(Duration::from_millis(ms)).await;
    ms
}

async fn in_start_order(delays: &[u64]) -> Vec<u64> {
    let handles: Vec<_> = delays.iter().map(|ms| tokio::spawn(delayed(*ms))).collect();
    let mut out = Vec::new();
    for handle in handles {
        out.push(handle.await.unwrap());
    }
    out
}

async fn in_finish_order(delays: &[u64]) -> Vec<u64> {
    todo!()
}

#[tokio::main]
async fn main() {
    println!("{:?}", in_start_order(&[30, 10, 20]).await);
    println!("{:?}", in_finish_order(&[30, 10, 20]).await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Instant;

    #[tokio::test(start_paused = true)]
    async fn handles_keep_start_order() {
        assert_eq!(in_start_order(&[30, 10, 20]).await, [30, 10, 20], "Vec<JoinHandle> ждётся в порядке запуска");
    }

    #[tokio::test(start_paused = true)]
    async fn join_set_gives_finish_order() {
        let started = Instant::now();
        assert_eq!(
            in_finish_order(&[30, 10, 20]).await,
            [10, 20, 30],
            "JoinSet должен отдавать сначала самую быструю задачу"
        );
        assert_eq!(started.elapsed(), Duration::from_millis(30), "задачи идут одновременно: всего 30 мс");
    }

    #[tokio::test(start_paused = true)]
    async fn empty_set() {
        assert!(in_finish_order(&[]).await.is_empty(), "пустой набор даёт пустой ответ");
    }
}
