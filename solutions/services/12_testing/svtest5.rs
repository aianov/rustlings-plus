use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::sleep;

fn main() {}

pub struct Mailer {
    latency: Duration,
    outbox: Arc<Mutex<Vec<String>>>,
    count: Arc<tokio::sync::watch::Sender<usize>>,
}

impl Mailer {
    pub fn new(latency: Duration) -> Mailer {
        Mailer { latency, outbox: Arc::default(), count: Arc::new(tokio::sync::watch::channel(0).0) }
    }

    pub fn send(&self, to: &str) {
        let (latency, outbox, count, to) = (self.latency, self.outbox.clone(), self.count.clone(), to.to_string());
        tokio::spawn(async move {
            sleep(latency).await;
            outbox.lock().unwrap().push(to);
            count.send_modify(|delivered| *delivered += 1);
        });
    }

    pub async fn wait_delivered(&self, n: usize) {
        let mut seen = self.count.subscribe();
        let _ = seen.wait_for(|delivered| *delivered >= n).await;
    }

    pub fn delivered(&self) -> Vec<String> {
        self.outbox.lock().unwrap().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{Instant, timeout};

    const MS: fn(u64) -> Duration = Duration::from_millis;

    #[tokio::test(start_paused = true)]
    async fn the_wait_ends_exactly_when_the_letter_is_delivered() {
        let mailer = Mailer::new(MS(250));
        let began = Instant::now();
        mailer.send("ann@example.com");
        mailer.wait_delivered(1).await;
        assert_eq!(began.elapsed(), MS(250), "письмо идёт 250 мс: ждать надо ровно столько, а не 100 (рано) и не секунду «с запасом»");
        assert_eq!(mailer.delivered(), vec!["ann@example.com".to_string()], "к моменту возврата письмо уже в ящике");
    }

    #[tokio::test(start_paused = true)]
    async fn fast_delivery_does_not_cost_the_slow_timeout() {
        let mailer = Mailer::new(MS(10));
        let began = Instant::now();
        mailer.send("a@x");
        mailer.wait_delivered(1).await;
        assert_eq!(began.elapsed(), MS(10), "быстрая доставка не должна ждать дольше самой доставки");
    }

    #[tokio::test(start_paused = true)]
    async fn waiters_for_different_counts_wake_at_their_own_moments() {
        let mailer = Arc::new(Mailer::new(MS(250)));
        let began = Instant::now();
        mailer.send("a@x");
        sleep(MS(100)).await;
        mailer.send("b@x");
        sleep(MS(100)).await;
        mailer.send("c@x");
        let (two, three) = (mailer.clone(), mailer.clone());
        let second = tokio::spawn(async move {
            two.wait_delivered(2).await;
            began.elapsed()
        });
        let third = tokio::spawn(async move {
            three.wait_delivered(3).await;
            began.elapsed()
        });
        assert_eq!(second.await.unwrap(), MS(350), "вторая доставка на 350-й миллисекунде (отправлено в 100, идёт 250)");
        assert_eq!(third.await.unwrap(), MS(450), "третья на 450-й (отправлено в 200)");
        assert_eq!(mailer.delivered().len(), 3, "все три в ящике");
    }

    #[tokio::test(start_paused = true)]
    async fn an_already_delivered_letter_needs_no_waiting() {
        let mailer = Mailer::new(MS(50));
        mailer.send("a@x");
        sleep(MS(500)).await;
        let before = Instant::now();
        mailer.wait_delivered(1).await;
        assert_eq!(before.elapsed(), Duration::ZERO, "доставлено давно: возвращаемся сразу, не засыпая заново");
    }

    #[tokio::test(start_paused = true)]
    async fn a_delivery_at_the_very_moment_of_the_wait_is_not_lost() {
        let mailer = Mailer::new(MS(250));
        let began = Instant::now();
        mailer.send("a@x");
        sleep(MS(250)).await;
        timeout(MS(1000), mailer.wait_delivered(1)).await.expect("письмо доставлено в тот же миг: ожидание не должно зависнуть");
        assert_eq!(began.elapsed(), MS(250), "время не ушло вперёд");
    }

    #[tokio::test(start_paused = true)]
    async fn waiting_for_more_than_will_ever_arrive_does_not_return_early() {
        let mailer = Mailer::new(MS(50));
        mailer.send("a@x");
        let result = timeout(MS(2000), mailer.wait_delivered(2)).await;
        assert!(result.is_err(), "отправлено одно письмо, значит, двух не дождаться: ожидание не имеет права вернуться по таймеру");
    }
}
