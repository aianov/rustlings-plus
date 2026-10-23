// Сервис состоит из частей, у каждой части свои задачи. Остановить всё сразу можно одним токеном,
// но иногда нужно погасить только одну часть, а остальные оставить. Для этого токены собирают в дерево:
// сигнал идёт вниз, от родителя к детям, и никогда вверх.
// TODO: Добавь `Token::child`. Отмена родителя отменяет детей и внуков, причём `is_cancelled`
// видит это сразу; отмена ребёнка не затрагивает ни родителя, ни его других детей.
use std::sync::Arc;
use tokio::sync::watch;

#[derive(Clone)]
struct Token {
    flag: Arc<watch::Sender<bool>>,
}

impl Token {
    fn new() -> Token {
        Token { flag: Arc::new(watch::channel(false).0) }
    }

    fn child(&self) -> Token {
        todo!()
    }

    fn cancel(&self) {
        self.flag.send_replace(true);
    }

    fn is_cancelled(&self) -> bool {
        *self.flag.borrow()
    }

    async fn cancelled(&self) {
        let mut rx = self.flag.subscribe();
        let _ = rx.wait_for(|cancelled| *cancelled).await;
    }
}

#[tokio::main]
async fn main() {
    let root = Token::new();
    let child = root.child();
    root.cancel();
    println!("{}", child.is_cancelled());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::{sleep, timeout};

    #[tokio::test]
    async fn parent_cancel_reaches_children_and_grandchildren_at_once() {
        let root = Token::new();
        let child = root.child();
        let grandchild = child.child();
        assert!(!child.is_cancelled() && !grandchild.is_cancelled(), "пока никто не отменён, дети живы");
        root.cancel();
        assert!(child.is_cancelled(), "ребёнок должен видеть отмену родителя сразу, без ожидания");
        assert!(grandchild.is_cancelled(), "внук тоже: сигнал идёт по всей цепочке");
    }

    #[tokio::test]
    async fn child_cancel_stays_below() {
        let root = Token::new();
        let left = root.child();
        let right = root.child();
        let grandchild = left.child();
        left.cancel();
        assert!(left.is_cancelled(), "ребёнок отменён");
        assert!(grandchild.is_cancelled(), "внук отменяется вместе с родителем");
        assert!(!root.is_cancelled(), "отмена ребёнка не должна подниматься к родителю");
        assert!(!right.is_cancelled(), "отмена одного ребёнка не затрагивает брата");
    }

    #[tokio::test(start_paused = true)]
    async fn waiting_on_a_child_wakes_up_on_parent_cancel() {
        let root = Token::new();
        let child = root.child();
        let waiter = tokio::spawn(async move { child.cancelled().await });
        sleep(Duration::from_millis(20)).await;
        assert!(!waiter.is_finished(), "никто не отменён, ждущий ждёт");
        root.cancel();
        timeout(Duration::from_secs(1), waiter)
            .await
            .expect("родитель отменён, а ребёнок так и не проснулся")
            .unwrap();
    }

    #[tokio::test]
    async fn a_child_of_a_cancelled_parent_is_born_cancelled() {
        let root = Token::new();
        root.cancel();
        let child = root.child();
        assert!(child.is_cancelled(), "ребёнок отменённого родителя рождается отменённым");
        timeout(Duration::from_secs(1), child.cancelled()).await.expect("cancelled() у такого ребёнка должен вернуться сразу");
    }
}
