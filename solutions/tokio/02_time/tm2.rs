use std::time::Duration;

async fn lookup(delay_ms: u64) -> String {
    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
    "найдено".to_string()
}

async fn lookup_or_default(delay_ms: u64) -> String {
    match tokio::time::timeout(Duration::from_millis(100), lookup(delay_ms)).await {
        Ok(found) => found,
        Err(_) => "по умолчанию".to_string(),
    }
}

async fn count_late(delays: &[u64]) -> usize {
    let mut late = 0;
    for delay in delays {
        if tokio::time::timeout(Duration::from_millis(100), lookup(*delay)).await.is_err() {
            late += 1;
        }
    }
    late
}

#[tokio::main]
async fn main() {
    println!("{}", lookup_or_default(50).await);
    println!("{}", lookup_or_default(500).await);
    println!("{}", count_late(&[10, 200]).await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Instant;

    #[tokio::test(start_paused = true)]
    async fn fast_answer_is_returned() {
        assert_eq!(lookup_or_default(50).await, "найдено", "успел за срок: настоящий ответ");
    }

    #[tokio::test(start_paused = true)]
    async fn slow_answer_gives_default_at_the_deadline() {
        let started = Instant::now();
        assert_eq!(lookup_or_default(150).await, "по умолчанию", "опоздал: значение по умолчанию");
        assert_eq!(
            started.elapsed(),
            Duration::from_millis(100),
            "ждать нужно ровно до срока (100 мс), а не до прихода позднего ответа"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn counts_only_late_ones() {
        assert_eq!(count_late(&[50, 150, 99, 400, 10]).await, 2, "опоздали 150 и 400");
        assert_eq!(count_late(&[]).await, 0, "пустой список без опозданий");
    }

    #[tokio::test(start_paused = true)]
    async fn late_ones_do_not_cost_full_time() {
        let started = Instant::now();
        count_late(&[1000, 1000, 1000]).await;
        assert_eq!(started.elapsed(), Duration::from_millis(300), "каждый опоздавший стоит ровно срок, не больше");
    }
}
