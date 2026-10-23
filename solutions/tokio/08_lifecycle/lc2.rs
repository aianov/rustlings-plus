use futures::future::select_all;
use std::sync::Arc;
use tokio::sync::watch;

#[derive(Clone)]
struct Token {
    own: Arc<watch::Sender<bool>>,
    parents: Vec<Arc<watch::Sender<bool>>>,
}

impl Token {
    fn new() -> Token {
        Token { own: Arc::new(watch::channel(false).0), parents: Vec::new() }
    }

    fn child(&self) -> Token {
        let mut parents = self.parents.clone();
        parents.push(self.own.clone());
        Token { own: Arc::new(watch::channel(false).0), parents }
    }

    fn cancel(&self) {
        self.own.send_replace(true);
    }

    fn is_cancelled(&self) -> bool {
        *self.own.borrow() || self.parents.iter().any(|flag| *flag.borrow())
    }

    async fn cancelled(&self) {
        let waits: Vec<_> = std::iter::once(&self.own)
            .chain(self.parents.iter())
            .map(|flag| {
                let mut rx = flag.subscribe();
                Box::pin(async move {
                    let _ = rx.wait_for(|cancelled| *cancelled).await;
                })
            })
            .collect();
        select_all(waits).await;
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
