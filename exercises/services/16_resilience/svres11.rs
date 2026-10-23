// user-service начал отвечать по минуте. Шлюз ходит во все сервисы через один общий пул на 8 одновременных вызовов. Медленные
// вызовы к user-service заняли все 8 мест и ждут, и теперь недоступен post-service, у которого всё в порядке: ему просто не досталось
// места. Так одна больная зависимость тянет за собой здоровые. Переборки на корабле нужны ровно для этого: пробоина в одном отсеке не
// затапливает остальные.
// Цель: у каждой зависимости свой лимит одновременных вызовов (`Client::new(&[("user", 4), ("post", 4)])`); сверх лимита вызов
// отвергается сразу, не вставая в очередь; место возвращается и при успехе, и при отмене; незнакомая зависимость это ошибка.


use std::future::Future;
use std::sync::Arc;
use tokio::sync::Semaphore;

fn main() {}

#[derive(Debug, PartialEq, Eq)]
pub enum Rejected {
    /// У зависимости заняты все места.
    Full,
    /// Такой зависимости нет в настройках.
    Unknown,
}

pub struct Client {
    shared: Arc<Semaphore>,
}

impl Client {
    pub fn new(limits: &[(&'static str, usize)]) -> Client {
        Client { shared: Arc::new(Semaphore::new(limits.iter().map(|(_, n)| n).sum())) }
    }

    pub async fn call<T>(&self, _dep: &'static str, work: impl Future<Output = T>) -> Result<T, Rejected> {
        let _place = self.shared.acquire().await.expect("семафор не закрывается");
        Ok(work.await)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::Instant;

    const SLOW: Duration = Duration::from_secs(60);

    fn client() -> Arc<Client> {
        Arc::new(Client::new(&[("user", 4), ("post", 4)]))
    }

    async fn clog_user(client: &Arc<Client>, n: usize) -> Vec<tokio::task::JoinHandle<Result<(), Rejected>>> {
        let tasks: Vec<_> = (0..n)
            .map(|_| {
                let c = client.clone();
                tokio::spawn(async move { c.call("user", tokio::time::sleep(SLOW)).await })
            })
            .collect();
        tokio::time::sleep(Duration::from_millis(1)).await;
        tasks
    }

    #[tokio::test(start_paused = true)]
    async fn a_slow_dependency_does_not_take_places_of_the_healthy_one() {
        let c = client();
        let _stuck = clog_user(&c, 8).await;
        let start = Instant::now();
        let r = c.call("post", async { 7 }).await;
        assert_eq!(r, Ok(7), "post-service здоров, а вызов к нему отвергнут: места общие с зависшим user-service");
        assert_eq!(start.elapsed(), Duration::ZERO, "вызов к post не должен был ждать");
    }

    #[tokio::test(start_paused = true)]
    async fn over_the_limit_a_call_is_rejected_at_once() {
        let c = client();
        let _stuck = clog_user(&c, 4).await;
        let start = Instant::now();
        let r = c.call("user", async { 1 }).await;
        assert_eq!(r, Err(Rejected::Full), "пятый вызов при лимите четыре должен получить отказ");
        assert_eq!(start.elapsed(), Duration::ZERO, "отказ должен быть мгновенным: стоять в очереди значит занимать воркера и ждать вместе с зависшими");
    }

    #[tokio::test(start_paused = true)]
    async fn places_come_back_after_calls_finish() {
        let c = client();
        let stuck = clog_user(&c, 4).await;
        for t in stuck {
            t.await.unwrap().unwrap();
        }
        assert_eq!(c.call("user", async { 5 }).await, Ok(5), "все вызовы закончились, места должны вернуться");
    }

    #[tokio::test(start_paused = true)]
    async fn places_come_back_when_a_call_is_cancelled() {
        let c = client();
        let stuck = clog_user(&c, 4).await;
        for t in &stuck {
            t.abort();
        }
        for t in stuck {
            let _ = t.await;
        }
        for _ in 0..4 {
            assert_eq!(c.call("user", async { 1 }).await, Ok(1), "отменённые вызовы не вернули места");
        }
    }

    #[tokio::test(start_paused = true)]
    async fn an_unknown_dependency_is_an_error() {
        let c = client();
        assert_eq!(c.call("billing", async { 1 }).await, Err(Rejected::Unknown));
    }
}
