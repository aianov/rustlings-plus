// Пользователь удаляет аккаунт, и вместе с ним надо удалить все его посты. У рекордсмена их миллион. Одна команда
// на миллион строк держит блокировки, раздувает журнал базы и, упав на девятисотом тысячном, откатывает всё
// сделанное. Цель: `delete_all_posts` удаляет посты пачками (не больше `batch` за раз, каждая пачка отдельной
// операцией), не загружает в память весь список, не занимает исполнитель надолго и после сбоя на середине при
// повторном запуске доделывает остаток, не трогая чужие посты. Возвращает, сколько постов удалено этим запуском.

use std::collections::BTreeMap;
use std::sync::Mutex;

fn main() {}

#[derive(Default)]
struct Inner {
    posts: BTreeMap<u64, String>,
    next_id: u64,
    delete_calls: usize,
    fail_on_call: Option<usize>,
    biggest_page: usize,
    biggest_delete: usize,
}

#[derive(Default)]
pub struct PostStore {
    inner: Mutex<Inner>,
}

impl PostStore {
    pub fn add(&self, author: &str, count: usize) {
        let mut inner = self.inner.lock().unwrap();
        for _ in 0..count {
            inner.next_id += 1;
            let id = inner.next_id;
            inner.posts.insert(id, author.to_string());
        }
    }

    pub fn page(&self, author: &str, limit: usize) -> Vec<u64> {
        let mut inner = self.inner.lock().unwrap();
        inner.biggest_page = inner.biggest_page.max(limit);
        inner.posts.iter().filter(|(_, a)| a.as_str() == author).map(|(id, _)| *id).take(limit).collect()
    }

    pub fn delete_batch(&self, ids: &[u64]) -> Result<usize, String> {
        let mut inner = self.inner.lock().unwrap();
        inner.delete_calls += 1;
        inner.biggest_delete = inner.biggest_delete.max(ids.len());
        if inner.fail_on_call == Some(inner.delete_calls) {
            return Err("база оборвала соединение".to_string());
        }
        Ok(ids.iter().filter(|id| inner.posts.remove(id).is_some()).count())
    }

    pub fn count(&self, author: &str) -> usize {
        self.inner.lock().unwrap().posts.values().filter(|a| a.as_str() == author).count()
    }

    pub fn fail_on_delete_call(&self, n: usize) {
        self.inner.lock().unwrap().fail_on_call = Some(n);
    }

    pub fn biggest_page(&self) -> usize {
        self.inner.lock().unwrap().biggest_page
    }

    pub fn biggest_delete(&self) -> usize {
        self.inner.lock().unwrap().biggest_delete
    }
}

pub async fn delete_all_posts(store: &PostStore, author: &str, batch: usize) -> Result<usize, String> {
    let _ = (store, author, batch);
    let _ = tokio::task::yield_now();
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn everything_of_the_author_goes_and_nothing_of_the_others() {
        let store = PostStore::default();
        store.add("alice", 2350);
        store.add("bob", 40);
        let deleted = delete_all_posts(&store, "alice", 500).await.expect("удаление");
        assert_eq!(deleted, 2350, "удалено ровно столько, сколько было у автора");
        assert_eq!(store.count("alice"), 0, "у автора ничего не осталось");
        assert_eq!(store.count("bob"), 40, "чужие посты на месте");
    }

    #[tokio::test]
    async fn the_work_is_cut_into_small_batches() {
        let store = PostStore::default();
        store.add("alice", 2350);
        delete_all_posts(&store, "alice", 500).await.unwrap();
        assert!(store.biggest_delete() <= 500, "одна операция удалила {} записей, а пачка не больше 500", store.biggest_delete());
        assert!(store.biggest_page() <= 500, "из базы запрошено {} id за раз: весь список в память класть нельзя, у рекордсмена их миллион", store.biggest_page());
    }

    #[tokio::test]
    async fn an_author_without_posts_is_done_at_once() {
        let store = PostStore::default();
        store.add("bob", 3);
        assert_eq!(delete_all_posts(&store, "alice", 100).await, Ok(0), "удалять нечего: ноль и без зацикливания");
    }

    #[tokio::test]
    async fn a_failure_in_the_middle_keeps_what_is_done_and_the_rerun_finishes() {
        let store = PostStore::default();
        store.add("alice", 1000);
        store.fail_on_delete_call(3);
        let first = delete_all_posts(&store, "alice", 100).await;
        assert!(first.is_err(), "ошибка базы должна дойти до вызывающего, чтобы задача повторилась: {first:?}");
        assert_eq!(store.count("alice"), 800, "две пачки уже удалены и остаются удалёнными, откатываться не нужно");
        let second = delete_all_posts(&store, "alice", 100).await.expect("повторный запуск");
        assert_eq!(second, 800, "повтор доделывает остаток и не считает уже удалённое");
        assert_eq!(store.count("alice"), 0, "в конце пусто");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn a_long_job_lets_other_tasks_breathe() {
        let store = PostStore::default();
        store.add("alice", 5000);
        let ticks = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&ticks);
        let ticker = tokio::spawn(async move {
            loop {
                counter.fetch_add(1, Ordering::SeqCst);
                tokio::task::yield_now().await;
            }
        });
        delete_all_posts(&store, "alice", 100).await.unwrap();
        ticker.abort();
        assert!(ticks.load(Ordering::SeqCst) >= 25, "за 50 пачек другая задача сделала всего {} шагов: между пачками нужно уступать исполнитель", ticks.load(Ordering::SeqCst));
    }
}
