// Лента из 50 постов показывает имя автора у каждого. Код делает для каждого поста вызов `users.get(author)`: пятьдесят вызовов по
// 5 мс друг за другом, четверть секунды только на имена. И это при том, что авторов в ленте восемь, остальные повторяются. Такой
// узор называется N+1: один запрос за списком и N запросов за подробностями к каждому элементу.
// Цель: `render_feed` ходит в `get_many` за всеми нужными авторами разом, спрашивает каждого автора один раз, учитывает, что за вызов
// можно запросить не больше 100 идентификаторов, сохраняет порядок постов, а неизвестного автора подписывает «?».


use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

fn main() {}

pub struct Users {
    names: HashMap<u64, String>,
    single_calls: AtomicU32,
    batch_calls: AtomicU32,
    batch_ids: AtomicU32,
}

impl Users {
    pub fn new(names: HashMap<u64, String>) -> Users {
        Users { names, single_calls: AtomicU32::new(0), batch_calls: AtomicU32::new(0), batch_ids: AtomicU32::new(0) }
    }

    pub fn single_calls(&self) -> u32 {
        self.single_calls.load(Ordering::SeqCst)
    }

    pub fn batch_calls(&self) -> u32 {
        self.batch_calls.load(Ordering::SeqCst)
    }

    pub fn batch_ids(&self) -> u32 {
        self.batch_ids.load(Ordering::SeqCst)
    }

    /// Один пользователь за вызов, 5 мс.
    pub async fn get(&self, id: u64) -> Option<String> {
        self.single_calls.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(5)).await;
        self.names.get(&id).cloned()
    }

    /// До 100 пользователей за вызов, те же 5 мс. Неизвестные в ответ не попадают.
    pub async fn get_many(&self, ids: &[u64]) -> HashMap<u64, String> {
        assert!(ids.len() <= 100, "за один вызов нельзя спрашивать больше 100 идентификаторов");
        self.batch_calls.fetch_add(1, Ordering::SeqCst);
        self.batch_ids.fetch_add(ids.len() as u32, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(5)).await;
        ids.iter().filter_map(|id| self.names.get(id).map(|n| (*id, n.clone()))).collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Post {
    pub id: u64,
    pub author: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Item {
    pub post: u64,
    pub author_name: String,
}

pub async fn render_feed(users: &Users, posts: &[Post]) -> Vec<Item> {
    let mut items = Vec::new();
    for post in posts {
        let name = users.get(post.author).await.unwrap_or_else(|| "?".to_string());
        items.push(Item { post: post.id, author_name: name });
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Instant;

    fn users(n: u64) -> Users {
        Users::new((1..=n).map(|i| (i, format!("автор{i}"))).collect())
    }

    fn posts(n: u64, authors: u64) -> Vec<Post> {
        (0..n).map(|i| Post { id: i, author: 1 + i % authors }).collect()
    }

    #[tokio::test(start_paused = true)]
    async fn fifty_posts_by_eight_authors_cost_one_call() {
        let u = users(8);
        let start = Instant::now();
        let items = render_feed(&u, &posts(50, 8)).await;
        assert_eq!(items.len(), 50);
        assert_eq!(items[10], Item { post: 10, author_name: "автор3".to_string() });
        assert_eq!(u.single_calls(), 0, "поштучные вызовы остались");
        assert_eq!(u.batch_calls(), 1, "вызовов за авторами {}, должен быть один", u.batch_calls());
        assert_eq!(u.batch_ids(), 8, "спрошено {} идентификаторов при восьми разных авторах", u.batch_ids());
        assert_eq!(start.elapsed(), Duration::from_millis(5));
    }

    #[tokio::test(start_paused = true)]
    async fn big_feeds_respect_the_batch_limit() {
        let u = users(250);
        let items = render_feed(&u, &posts(250, 250)).await;
        assert_eq!(items.len(), 250);
        assert_eq!(u.batch_calls(), 3, "250 разных авторов по 100 за вызов: три вызова");
        assert_eq!(items[249].author_name, "автор250");
    }

    #[tokio::test(start_paused = true)]
    async fn order_is_kept_and_unknown_authors_are_marked() {
        let u = users(2);
        let feed = vec![Post { id: 5, author: 2 }, Post { id: 4, author: 99 }, Post { id: 3, author: 1 }, Post { id: 2, author: 99 }];
        let items = render_feed(&u, &feed).await;
        let names: Vec<_> = items.iter().map(|i| (i.post, i.author_name.as_str())).collect();
        assert_eq!(names, vec![(5, "автор2"), (4, "?"), (3, "автор1"), (2, "?")]);
        assert_eq!(u.batch_ids(), 3);
    }

    #[tokio::test(start_paused = true)]
    async fn an_empty_feed_calls_nobody() {
        let u = users(3);
        assert!(render_feed(&u, &[]).await.is_empty());
        assert_eq!((u.single_calls(), u.batch_calls()), (0, 0));
    }
}
