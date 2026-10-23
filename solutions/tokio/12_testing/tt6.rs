use std::sync::Mutex;
use std::time::Duration;
use tokio::time::sleep;

trait Bank: Send + Sync + 'static {
    fn open(a: u64, b: u64) -> Self;
    fn transfer(&self, amount: u64) -> impl Future<Output = ()> + Send;
    fn balances(&self) -> (u64, u64);
}

struct Good(Mutex<(u64, u64)>);
struct DebitFirst(Mutex<(u64, u64)>);
struct CreditFirst(Mutex<(u64, u64)>);

impl Bank for Good {
    fn open(a: u64, b: u64) -> Good {
        Good(Mutex::new((a, b)))
    }

    async fn transfer(&self, amount: u64) {
        sleep(Duration::from_millis(10)).await;
        let mut accounts = self.0.lock().unwrap();
        accounts.0 -= amount;
        accounts.1 += amount;
    }

    fn balances(&self) -> (u64, u64) {
        *self.0.lock().unwrap()
    }
}

impl Bank for DebitFirst {
    fn open(a: u64, b: u64) -> DebitFirst {
        DebitFirst(Mutex::new((a, b)))
    }

    async fn transfer(&self, amount: u64) {
        self.0.lock().unwrap().0 -= amount;
        sleep(Duration::from_millis(10)).await;
        self.0.lock().unwrap().1 += amount;
    }

    fn balances(&self) -> (u64, u64) {
        *self.0.lock().unwrap()
    }
}

impl Bank for CreditFirst {
    fn open(a: u64, b: u64) -> CreditFirst {
        CreditFirst(Mutex::new((a, b)))
    }

    async fn transfer(&self, amount: u64) {
        self.0.lock().unwrap().1 += amount;
        sleep(Duration::from_millis(10)).await;
        self.0.lock().unwrap().0 -= amount;
    }

    fn balances(&self) -> (u64, u64) {
        *self.0.lock().unwrap()
    }
}

#[derive(Debug, PartialEq)]
enum Verdict {
    Passed,
    Failed,
    Hung,
}

fn verdict<F: FnOnce() + Send + 'static>(secs: u64, run: F) -> Verdict {
    let (done, finished) = std::sync::mpsc::channel();
    let worker = std::thread::Builder::new()
        .spawn(move || {
            run();
            let _ = done.send(());
        })
        .unwrap();
    match finished.recv_timeout(Duration::from_secs(secs)) {
        Ok(()) => {
            worker.join().unwrap();
            Verdict::Passed
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Verdict::Failed,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Verdict::Hung,
    }
}

fn suite<B: Bank>() {
    let rt = tokio::runtime::Builder::new_current_thread().enable_time().start_paused(true).build().unwrap();
    rt.block_on(async {
        let bank = B::open(100, 0);
        bank.transfer(30).await;
        assert_eq!(bank.balances(), (70, 30), "завершённый перевод на 30 должен переложить деньги");
        for cut in 0..=15u64 {
            let bank = B::open(100, 0);
            let _ = tokio::time::timeout(Duration::from_millis(cut), bank.transfer(30)).await;
            let (a, b) = bank.balances();
            assert_eq!(a + b, 100, "отмена на {cut} мс потеряла или создала деньги: {a} и {b}");
        }
    });
}

fn main() {
    suite::<Good>();
    println!("Good проходит");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_suite_passes_on_the_correct_bank() {
        assert_eq!(verdict(20, suite::<Good>), Verdict::Passed, "на исправном банке тест обязан проходить");
    }

    #[test]
    fn the_suite_notices_a_debit_without_a_credit() {
        assert_eq!(verdict(20, suite::<DebitFirst>), Verdict::Failed, "отмена после списания теряет деньги: тест обязан это заметить");
    }

    #[test]
    fn the_suite_notices_a_credit_without_a_debit() {
        assert_eq!(verdict(20, suite::<CreditFirst>), Verdict::Failed, "отмена после зачисления создаёт деньги: тест обязан это заметить");
    }
}
