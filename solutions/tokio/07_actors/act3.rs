use std::collections::HashMap;
use tokio::sync::{mpsc, oneshot};

enum Cmd {
    Add { user: u32, url: String, reply: oneshot::Sender<bool> },
    Remove { user: u32, url: String, reply: oneshot::Sender<bool> },
    List { user: u32, reply: oneshot::Sender<Vec<String>> },
}

async fn actor(mut rx: mpsc::Receiver<Cmd>) {
    let mut marks: HashMap<u32, Vec<String>> = HashMap::new();
    while let Some(cmd) = rx.recv().await {
        match cmd {
            Cmd::Add { user, url, reply } => {
                let list = marks.entry(user).or_default();
                let fresh = !list.contains(&url);
                if fresh {
                    list.push(url);
                }
                let _ = reply.send(fresh);
            }
            Cmd::Remove { user, url, reply } => {
                let list = marks.entry(user).or_default();
                let before = list.len();
                list.retain(|u| *u != url);
                let _ = reply.send(list.len() != before);
            }
            Cmd::List { user, reply } => {
                let _ = reply.send(marks.get(&user).cloned().unwrap_or_default());
            }
        }
    }
}

#[derive(Clone)]
struct Bookmarks {
    tx: mpsc::Sender<Cmd>,
}

impl Bookmarks {
    fn start() -> Bookmarks {
        let (tx, rx) = mpsc::channel(32);
        tokio::spawn(actor(rx));
        Bookmarks { tx }
    }

    async fn add(&self, user: u32, url: &str) -> bool {
        let (reply, answer) = oneshot::channel();
        let cmd = Cmd::Add { user, url: url.to_string(), reply };
        self.tx.send(cmd).await.expect("актор закладок остановился");
        answer.await.expect("актор закладок не ответил")
    }

    async fn remove(&self, user: u32, url: &str) -> bool {
        let (reply, answer) = oneshot::channel();
        let cmd = Cmd::Remove { user, url: url.to_string(), reply };
        self.tx.send(cmd).await.expect("актор закладок остановился");
        answer.await.expect("актор закладок не ответил")
    }

    async fn list(&self, user: u32) -> Vec<String> {
        let (reply, answer) = oneshot::channel();
        self.tx.send(Cmd::List { user, reply }).await.expect("актор закладок остановился");
        answer.await.expect("актор закладок не ответил")
    }
}

#[tokio::main]
async fn main() {
    let marks = Bookmarks::start();
    marks.add(1, "https://rust-lang.org").await;
    println!("{:?}", marks.list(1).await);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_handle<T: Clone + Send + Sync + 'static>() {}

    #[tokio::test]
    async fn add_list_remove() {
        let marks = Bookmarks::start();
        assert!(marks.add(1, "a").await, "первая закладка новая");
        assert!(marks.add(1, "b").await, "вторая закладка тоже новая");
        assert!(!marks.add(1, "a").await, "повтор той же ссылки не добавляется и сообщает false");
        assert_eq!(marks.list(1).await, vec!["a", "b"], "список в порядке добавления, без повторов");
        assert!(marks.remove(1, "a").await, "удалили то, что было");
        assert!(!marks.remove(1, "a").await, "удалять нечего");
        assert_eq!(marks.list(1).await, vec!["b"], "осталась одна закладка");
    }

    #[tokio::test]
    async fn users_are_independent() {
        let marks = Bookmarks::start();
        marks.add(1, "a").await;
        marks.add(2, "b").await;
        assert_eq!(marks.list(1).await, vec!["a"], "закладки пользователя 1");
        assert_eq!(marks.list(2).await, vec!["b"], "закладки пользователя 2");
        assert!(marks.list(3).await.is_empty(), "у пользователя 3 закладок нет");
    }

    #[tokio::test]
    async fn clones_share_one_actor_and_work_from_other_tasks() {
        assert_handle::<Bookmarks>();
        let marks = Bookmarks::start();
        let mut tasks = Vec::new();
        for user in 0..5u32 {
            let marks = marks.clone();
            tasks.push(tokio::spawn(async move {
                marks.add(user, "https://example.org").await;
            }));
        }
        for task in tasks {
            task.await.unwrap();
        }
        for user in 0..5u32 {
            assert_eq!(marks.list(user).await.len(), 1, "закладка пользователя {user}, добавленная из другой задачи, должна быть видна");
        }
    }
}
