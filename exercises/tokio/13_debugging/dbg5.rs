// Перевод между счетами берёт два замка: сначала списываемого, потом зачисляемого. Пока переводы
// идут по одному, всё в порядке. Но два встречных перевода (A в B и B в A) одновременно встают
// навсегда. Ещё перевод самому себе тоже встаёт. Почини `transfer` так, чтобы любой набор
// переводов одновременно завершался, а деньги не терялись и не появлялись.
use tokio::sync::Mutex;

struct Bank {
    accounts: Vec<Mutex<i64>>,
}

impl Bank {
    fn new(balances: &[i64]) -> Bank {
        Bank { accounts: balances.iter().map(|b| Mutex::new(*b)).collect() }
    }

    async fn balance(&self, id: usize) -> i64 {
        *self.accounts[id].lock().await
    }

    async fn transfer(&self, from: usize, to: usize, amount: i64) {
        let mut source = self.accounts[from].lock().await;
        tokio::task::yield_now().await;
        let mut target = self.accounts[to].lock().await;
        *source -= amount;
        *target += amount;
    }
}

#[tokio::main]
async fn main() {
    let bank = Bank::new(&[100, 50]);
    bank.transfer(0, 1, 30).await;
    println!("{} {}", bank.balance(0).await, bank.balance(1).await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::time::timeout;

    async fn all_done(jobs: Vec<tokio::task::JoinHandle<()>>) {
        for job in jobs {
            timeout(Duration::from_secs(2), job).await.expect("переводы встали и не заканчиваются: замки ждут друг друга по кругу").unwrap();
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_single_transfer_moves_money() {
        let bank = Bank::new(&[100, 50]);
        bank.transfer(0, 1, 30).await;
        assert_eq!((bank.balance(0).await, bank.balance(1).await), (70, 80), "30 перешли с первого счёта на второй");
    }

    #[tokio::test(start_paused = true)]
    async fn opposite_transfers_finish() {
        let bank = Arc::new(Bank::new(&[100, 100]));
        let (b1, b2) = (bank.clone(), bank.clone());
        let jobs = vec![tokio::spawn(async move { b1.transfer(0, 1, 10).await }), tokio::spawn(async move { b2.transfer(1, 0, 25).await })];
        all_done(jobs).await;
        assert_eq!((bank.balance(0).await, bank.balance(1).await), (115, 85), "встречные переводы взаимно учтены");
    }

    #[tokio::test(start_paused = true)]
    async fn a_ring_of_transfers_finishes() {
        let bank = Arc::new(Bank::new(&[100; 5]));
        let jobs: Vec<_> = (0..5)
            .map(|i| {
                let bank = bank.clone();
                tokio::spawn(async move { bank.transfer(i, (i + 1) % 5, 10 * (i as i64 + 1)).await })
            })
            .collect();
        all_done(jobs).await;
        let mut total = 0;
        for id in 0..5 {
            total += bank.balance(id).await;
        }
        assert_eq!(total, 500, "сумма на счетах не меняется");
        assert_eq!(bank.balance(0).await, 100 - 10 + 50, "первый счёт отдал 10 и получил 50");
    }

    #[tokio::test(start_paused = true)]
    async fn a_transfer_to_yourself_changes_nothing() {
        let bank = Bank::new(&[100, 50]);
        timeout(Duration::from_secs(1), bank.transfer(1, 1, 20)).await.expect("перевод самому себе завис");
        assert_eq!(bank.balance(1).await, 50, "перевод самому себе ничего не меняет");
    }
}
