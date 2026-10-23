// Перевод денег между двумя счетами. Между списанием и зачислением нужно дождаться подтверждения
// банка (оно медленное). Если вызывающий обернёт перевод в timeout и срок выйдет посередине,
// Future будет дропнут прямо на .await, и то, что успело выполниться, останется навсегда.
// Сейчас в такой момент деньги исчезают. Перевод должен либо произойти целиком, либо не
// произойти вовсе, как бы рано его ни оборвали.
// TODO: Почини `transfer`. Сигнатуру и подтверждение `confirm` не трогай.
use std::sync::Mutex;
use std::time::Duration;

struct Bank {
    a: Mutex<i64>,
    b: Mutex<i64>,
}

impl Bank {
    fn new(a: i64, b: i64) -> Self {
        Bank { a: Mutex::new(a), b: Mutex::new(b) }
    }

    fn balances(&self) -> (i64, i64) {
        (*self.a.lock().unwrap(), *self.b.lock().unwrap())
    }
}

async fn confirm() {
    tokio::time::sleep(Duration::from_millis(100)).await;
}

async fn transfer(bank: &Bank, amount: i64) {
    *bank.a.lock().unwrap() -= amount;
    confirm().await;
    *bank.b.lock().unwrap() += amount;
}

#[tokio::main]
async fn main() {
    let bank = Bank::new(100, 0);
    transfer(&bank, 30).await;
    println!("{:?}", bank.balances());
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::timeout;

    #[tokio::test(start_paused = true)]
    async fn full_transfer() {
        let bank = Bank::new(100, 0);
        transfer(&bank, 30).await;
        assert_eq!(bank.balances(), (70, 30), "без обрыва перевод проходит целиком");
    }

    #[tokio::test(start_paused = true)]
    async fn cancelled_transfer_changes_nothing() {
        let bank = Bank::new(100, 0);
        let result = timeout(Duration::from_millis(50), transfer(&bank, 30)).await;
        assert!(result.is_err(), "за 50 мс подтверждение не придёт, перевод оборван");
        assert_eq!(
            bank.balances(),
            (100, 0),
            "деньги исчезли: списание выполнилось, а зачисление нет. Оборванный перевод не должен оставлять следов"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn money_is_never_lost_whenever_it_is_cut() {
        for cut_ms in (0..=150).step_by(10) {
            let bank = Bank::new(100, 0);
            let _ = timeout(Duration::from_millis(cut_ms), transfer(&bank, 30)).await;
            let (a, b) = bank.balances();
            assert_eq!(a + b, 100, "при обрыве на {cut_ms} мс сумма на счетах изменилась: {a} + {b}");
            assert!(
                (a, b) == (100, 0) || (a, b) == (70, 30),
                "при обрыве на {cut_ms} мс перевод остался наполовину: {a} и {b}"
            );
        }
    }
}
