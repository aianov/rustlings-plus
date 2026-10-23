// Дверь: задача ждёт сигнала «открыть», а другая его подаёт. Для одноразового сигнала между
// задачами в tokio есть Notify. Важно, как он ведёт себя в трёх ситуациях: сигнал подали раньше,
// чем задача начала ждать (он не должен пропасть), ждущих несколько (каждый сигнал будит одну),
// ждущая задача ещё не получила сигнал (она честно спит).
// TODO: Допиши `spawn_waiter` (задача ждёт сигнала и возвращает "дверь открыта") и `open`.
use std::sync::Arc;
use tokio::sync::Notify;
use tokio::task::JoinHandle;

fn spawn_waiter(door: Arc<Notify>) -> JoinHandle<&'static str> {
    todo!()
}

fn open(door: &Notify) {
    todo!()
}

#[tokio::main]
async fn main() {
    let door = Arc::new(Notify::new());
    let waiter = spawn_waiter(Arc::clone(&door));
    open(&door);
    println!("{}", waiter.await.unwrap());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test(start_paused = true)]
    async fn waiter_sleeps_until_opened() {
        let door = Arc::new(Notify::new());
        let waiter = spawn_waiter(Arc::clone(&door));
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert!(!waiter.is_finished(), "пока никто не открыл дверь, ждущая задача спит");
        open(&door);
        assert_eq!(waiter.await.unwrap(), "дверь открыта", "после сигнала задача просыпается");
    }

    #[tokio::test(start_paused = true)]
    async fn signal_given_before_waiting_is_not_lost() {
        let door = Arc::new(Notify::new());
        open(&door);
        let waiter = spawn_waiter(Arc::clone(&door));
        let result = tokio::time::timeout(Duration::from_secs(1), waiter).await;
        assert!(result.is_ok(), "сигнал подали раньше, чем задача стала ждать: он должен дождаться её, а не пропасть");
    }

    #[tokio::test(start_paused = true)]
    async fn one_signal_wakes_one_waiter() {
        let door = Arc::new(Notify::new());
        let first = spawn_waiter(Arc::clone(&door));
        let second = spawn_waiter(Arc::clone(&door));
        tokio::time::sleep(Duration::from_millis(10)).await;
        open(&door);
        tokio::time::sleep(Duration::from_millis(10)).await;
        let finished = usize::from(first.is_finished()) + usize::from(second.is_finished());
        assert_eq!(finished, 1, "один сигнал должен разбудить ровно одну задачу, а разбудил {finished}");
        open(&door);
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(first.is_finished() && second.is_finished(), "второй сигнал будит вторую задачу");
    }
}
