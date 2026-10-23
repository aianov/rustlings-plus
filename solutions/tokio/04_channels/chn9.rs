use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast;

struct Post {
    id: u32,
    body: String,
    clones: Arc<AtomicUsize>,
}

impl Clone for Post {
    fn clone(&self) -> Self {
        self.clones.fetch_add(1, Ordering::SeqCst);
        Post { id: self.id, body: self.body.clone(), clones: Arc::clone(&self.clones) }
    }
}

struct Hub {
    tx: broadcast::Sender<Arc<Post>>,
}

struct Subscriber {
    rx: broadcast::Receiver<Arc<Post>>,
}

impl Hub {
    fn new(capacity: usize) -> Hub {
        let (tx, _) = broadcast::channel(capacity);
        Hub { tx }
    }

    fn subscribe(&self) -> Subscriber {
        Subscriber { rx: self.tx.subscribe() }
    }

    fn publish(&self, post: Post) -> usize {
        self.tx.send(Arc::new(post)).unwrap_or(0)
    }
}

impl Subscriber {
    async fn next(&mut self) -> Option<Arc<Post>> {
        loop {
            match self.rx.recv().await {
                Ok(post) => return Some(post),
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let clones = Arc::new(AtomicUsize::new(0));
    let hub = Hub::new(8);
    let mut sub = hub.subscribe();
    hub.publish(Post { id: 1, body: "привет".into(), clones });
    println!("{}", sub.next().await.unwrap().body);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn post(id: u32, clones: &Arc<AtomicUsize>) -> Post {
        Post { id, body: "x".repeat(1000), clones: Arc::clone(clones) }
    }

    #[tokio::test]
    async fn everybody_gets_everything_without_copying_bodies() {
        let clones = Arc::new(AtomicUsize::new(0));
        let hub = Hub::new(16);
        let mut subs = [hub.subscribe(), hub.subscribe(), hub.subscribe()];
        for id in 0..5 {
            assert_eq!(hub.publish(post(id, &clones)), 3, "пост должен уйти трём подписчикам");
        }
        drop(hub);
        for sub in &mut subs {
            let mut ids = Vec::new();
            while let Some(p) = sub.next().await {
                ids.push(p.id);
            }
            assert_eq!(ids, vec![0, 1, 2, 3, 4], "каждый подписчик получает все посты по порядку");
        }
        assert_eq!(
            clones.load(Ordering::SeqCst),
            0,
            "тело поста скопировано {} раз: рассылай один общий экземпляр, а не копию на каждого подписчика",
            clones.load(Ordering::SeqCst)
        );
    }

    #[tokio::test]
    async fn publish_without_subscribers_is_fine() {
        let clones = Arc::new(AtomicUsize::new(0));
        let hub = Hub::new(4);
        assert_eq!(hub.publish(post(1, &clones)), 0, "подписчиков нет: ушло нулю, и это не ошибка и не паника");
    }

    #[tokio::test]
    async fn late_subscriber_sees_only_new_posts() {
        let clones = Arc::new(AtomicUsize::new(0));
        let hub = Hub::new(4);
        let mut early = hub.subscribe();
        hub.publish(post(1, &clones));
        let mut late = hub.subscribe();
        hub.publish(post(2, &clones));
        drop(hub);
        assert_eq!(early.next().await.unwrap().id, 1, "ранний подписчик видит первый пост");
        assert_eq!(late.next().await.unwrap().id, 2, "поздний начинает с поста, пришедшего после его подписки");
        assert!(late.next().await.is_none(), "хаб закрыт: больше постов нет");
    }
}
