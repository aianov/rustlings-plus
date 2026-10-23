// Мини-банк на мьютексах: по замку на счёт. Перевод берёт замок отправителя, делает короткую антифрод-
// проверку и берёт замок получателя. Когда Аня переводит Борису и одновременно Борис Ане, каждый
// держит свой замок и ждёт чужой. Это дедлок, и тест на нагрузке его увидит за 20 миллисекунд.
// TODO: Почини `transfer`, чтобы любые одновременные переводы заканчивались. Деньги не теряются
// и не создаются, а перевод самому себе (`from == to`) не должен зависать.
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

#[derive(Debug, PartialEq)]
pub enum TransferError {
    NoAccount,
    NoMoney,
}

pub struct Bank {
    accounts: Vec<Mutex<i64>>,
}

const FRAUD_CHECK: Duration = Duration::from_millis(20);

impl Bank {
    pub fn new(balances: &[i64]) -> Self {
        Bank { accounts: balances.iter().map(|&b| Mutex::new(b)).collect() }
    }

    pub fn balance(&self, account: usize) -> Option<i64> {
        self.accounts.get(account).map(|m| *m.lock().unwrap())
    }

    pub fn total(&self) -> i64 {
        self.accounts.iter().map(|m| *m.lock().unwrap()).sum()
    }

    pub fn transfer(&self, from: usize, to: usize, amount: i64) -> Result<(), TransferError> {
        let mut source = self.accounts.get(from).ok_or(TransferError::NoAccount)?.lock().unwrap();
        thread::sleep(FRAUD_CHECK);
        let mut target = self.accounts.get(to).ok_or(TransferError::NoAccount)?.lock().unwrap();
        if *source < amount {
            return Err(TransferError::NoMoney);
        }
        *source -= amount;
        *target += amount;
        Ok(())
    }
}

fn main() {
    let bank = Bank::new(&[100, 50]);
    println!("{:?} {:?}", bank.transfer(0, 1, 30), bank.total());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};

    fn within<T: Send + 'static>(secs: u64, what: &str, f: impl FnOnce() -> T + Send + 'static) -> T {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(f());
        });
        match rx.recv_timeout(std::time::Duration::from_secs(secs)) {
            Ok(value) => value,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => panic!("{what}: не уложилось в {secs} с, похоже на зависание"),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => panic!("{what}: поток упал с паникой"),
        }
    }

    #[test]
    fn a_simple_transfer_moves_money() {
        within(3, "перевод", || {
            let bank = Bank::new(&[100, 50]);
            assert_eq!(bank.transfer(0, 1, 30), Ok(()));
            assert_eq!((bank.balance(0), bank.balance(1)), (Some(70), Some(80)));
            assert_eq!(bank.transfer(1, 0, 500), Err(TransferError::NoMoney));
            assert_eq!(bank.transfer(1, 9, 1), Err(TransferError::NoAccount));
            assert_eq!(bank.transfer(9, 1, 1), Err(TransferError::NoAccount));
            assert_eq!(bank.total(), 150, "неудачные переводы ничего не меняют");
        });
    }

    #[test]
    fn opposite_transfers_do_not_deadlock() {
        let total = within(3, "встречные переводы", || {
            let bank = Arc::new(Bank::new(&[100, 100]));
            let barrier = Arc::new(Barrier::new(2));
            let handles: Vec<_> = [(0, 1), (1, 0)]
                .into_iter()
                .map(|(from, to)| {
                    let bank = Arc::clone(&bank);
                    let barrier = Arc::clone(&barrier);
                    std::thread::spawn(move || {
                        barrier.wait();
                        bank.transfer(from, to, 10).unwrap();
                    })
                })
                .collect();
            for handle in handles {
                handle.join().unwrap();
            }
            bank.total()
        });
        assert_eq!(total, 200);
    }

    #[test]
    fn a_ring_of_transfers_finishes_and_keeps_the_money() {
        let balances = within(4, "кольцо переводов", || {
            let bank = Arc::new(Bank::new(&[100, 100, 100, 100]));
            let barrier = Arc::new(Barrier::new(4));
            let handles: Vec<_> = (0..4)
                .map(|from| {
                    let bank = Arc::clone(&bank);
                    let barrier = Arc::clone(&barrier);
                    std::thread::spawn(move || {
                        barrier.wait();
                        bank.transfer(from, (from + 1) % 4, 10).unwrap();
                    })
                })
                .collect();
            for handle in handles {
                handle.join().unwrap();
            }
            (0..4).map(|i| bank.balance(i).unwrap()).collect::<Vec<_>>()
        });
        assert_eq!(balances, vec![100, 100, 100, 100], "по кругу каждый отдал 10 и получил 10");
    }

    #[test]
    fn transfer_to_yourself_neither_hangs_nor_changes_anything() {
        within(3, "перевод себе", || {
            let bank = Bank::new(&[40]);
            assert_eq!(bank.transfer(0, 0, 10), Ok(()));
            assert_eq!(bank.transfer(0, 0, 100), Err(TransferError::NoMoney));
            assert_eq!(bank.balance(0), Some(40));
        });
    }
}
