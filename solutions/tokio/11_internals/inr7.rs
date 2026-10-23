async fn next_record(i: u32) -> u64 {
    u64::from(i) * 3
}

async fn checksum(records: u32) -> u64 {
    let mut sum = 0;
    for i in 0..records {
        sum += next_record(i).await;
        tokio::task::consume_budget().await;
    }
    sum
}

#[tokio::main]
async fn main() {
    println!("{}", checksum(10).await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[tokio::test]
    async fn the_sum_stays_correct() {
        assert_eq!(checksum(1000).await, 3 * 499_500, "сумма 3*i по 1000 записям");
        assert_eq!(checksum(0).await, 0, "пустая сверка даёт ноль");
    }

    #[tokio::test]
    async fn neighbours_get_their_turn_during_a_long_checksum() {
        let ticks = Arc::new(AtomicU32::new(0));
        let t = ticks.clone();
        let neighbour = tokio::spawn(async move {
            loop {
                t.fetch_add(1, Ordering::SeqCst);
                tokio::task::yield_now().await;
            }
        });
        let seen = tokio::spawn({
            let ticks = ticks.clone();
            async move {
                let sum = checksum(1000).await;
                (sum, ticks.load(Ordering::SeqCst))
            }
        })
        .await
        .unwrap();
        neighbour.abort();
        assert_eq!(seen.0, 3 * 499_500, "результат не должен измениться");
        assert!(seen.1 >= 5, "за 1000 записей сосед проработал {} раз: сверка не уступает ему место", seen.1);
        assert!(seen.1 <= 120, "сосед проработал {} раз: сверка уступает на каждой записи, а это дорого", seen.1);
    }
}
