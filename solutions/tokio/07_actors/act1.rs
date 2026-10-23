use std::collections::HashMap;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

#[derive(Debug)]
enum Cmd {
    Like(u32),
    Unlike(u32),
}

fn spawn_likes() -> (mpsc::Sender<Cmd>, JoinHandle<HashMap<u32, u32>>) {
    let (tx, mut rx) = mpsc::channel(16);
    let task = tokio::spawn(async move {
        let mut likes: HashMap<u32, u32> = HashMap::new();
        while let Some(cmd) = rx.recv().await {
            match cmd {
                Cmd::Like(post) => *likes.entry(post).or_insert(0) += 1,
                Cmd::Unlike(post) => {
                    if let Some(count) = likes.get_mut(&post) {
                        *count -= 1;
                        if *count == 0 {
                            likes.remove(&post);
                        }
                    }
                }
            }
        }
        likes
    });
    (tx, task)
}

#[tokio::main]
async fn main() {
    let (tx, task) = spawn_likes();
    tx.send(Cmd::Like(1)).await.unwrap();
    drop(tx);
    println!("{:?}", task.await.unwrap());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn counts_likes_per_post() {
        let (tx, task) = spawn_likes();
        for post in [1, 2, 1, 1, 3, 2] {
            tx.send(Cmd::Like(post)).await.unwrap();
        }
        drop(tx);
        let likes = task.await.unwrap();
        assert_eq!(likes.get(&1), Some(&3), "у поста 1 три лайка");
        assert_eq!(likes.get(&2), Some(&2), "у поста 2 два лайка");
        assert_eq!(likes.get(&3), Some(&1), "у поста 3 один лайк");
    }

    #[tokio::test]
    async fn unlike_never_goes_below_zero() {
        let (tx, task) = spawn_likes();
        tx.send(Cmd::Unlike(9)).await.unwrap();
        tx.send(Cmd::Like(5)).await.unwrap();
        tx.send(Cmd::Unlike(5)).await.unwrap();
        tx.send(Cmd::Unlike(5)).await.unwrap();
        tx.send(Cmd::Like(6)).await.unwrap();
        drop(tx);
        let likes = task.await.unwrap();
        assert_eq!(likes.get(&9), None, "дизлайк поста без лайков не заводит запись");
        assert_eq!(likes.get(&5), None, "лишний дизлайк не уводит счётчик в минус, а пост без лайков записи не имеет");
        assert_eq!(likes.len(), 1, "в таблице должен остаться только пост 6");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn many_senders_lose_nothing() {
        let (tx, task) = spawn_likes();
        let mut senders = Vec::new();
        for _ in 0..10 {
            let tx = tx.clone();
            senders.push(tokio::spawn(async move {
                for _ in 0..100 {
                    tx.send(Cmd::Like(7)).await.unwrap();
                }
            }));
        }
        drop(tx);
        for sender in senders {
            sender.await.unwrap();
        }
        let likes = task.await.unwrap();
        assert_eq!(likes.get(&7), Some(&1000), "десять задач по сто лайков: потерь быть не должно");
    }

    #[tokio::test]
    async fn actor_lives_until_the_last_sender_is_gone() {
        let (tx, task) = spawn_likes();
        tx.send(Cmd::Like(1)).await.unwrap();
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
        assert!(!task.is_finished(), "отправитель ещё жив, задача не должна завершаться");
        let second = tx.clone();
        drop(tx);
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
        assert!(!task.is_finished(), "остался клон отправителя, задача обязана работать дальше");
        second.send(Cmd::Like(1)).await.unwrap();
        drop(second);
        let likes = task.await.unwrap();
        assert_eq!(likes.get(&1), Some(&2), "лайки от обоих отправителей учтены");
    }
}
