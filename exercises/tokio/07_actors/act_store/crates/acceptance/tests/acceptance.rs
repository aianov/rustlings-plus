use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::FutureExt;
use futures::future::BoxFuture;
use store::{Config, Disk, Post, Store};
use tokio::time::{Instant, sleep};

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

struct SlowDisk {
    delay: Duration,
    start: Instant,
    writes: Mutex<Vec<(Duration, Vec<Post>)>>,
}

impl SlowDisk {
    fn new(delay_ms: u64) -> Arc<SlowDisk> {
        Arc::new(SlowDisk { delay: ms(delay_ms), start: Instant::now(), writes: Mutex::new(Vec::new()) })
    }

    fn batches(&self) -> Vec<(Duration, Vec<u64>)> {
        self.writes.lock().unwrap().iter().map(|(at, batch)| (*at, batch.iter().map(|p| p.id).collect())).collect()
    }

    fn all_ids(&self) -> Vec<u64> {
        self.batches().into_iter().flat_map(|(_, ids)| ids).collect()
    }
}

impl Disk for SlowDisk {
    fn write(&self, batch: Vec<Post>) -> BoxFuture<'_, ()> {
        async move {
            let at = self.start.elapsed();
            sleep(self.delay).await;
            self.writes.lock().unwrap().push((at, batch));
        }
        .boxed()
    }
}

fn config() -> Config {
    Config { batch_max: 3, batch_wait: ms(100) }
}

fn assert_handle<T: Clone + Send + Sync + 'static>() {}

#[tokio::test(start_paused = true)]
async fn ids_are_sequential_and_posts_are_readable() {
    assert_handle::<Store>();
    let (store, _runner) = Store::start(SlowDisk::new(10), config());
    let first = store.publish(7, "привет").await.unwrap();
    let second = store.publish(8, "и тебе").await.unwrap();
    let third = store.publish(7, "ещё один").await.unwrap();
    assert_eq!((first.id, second.id, third.id), (1, 2, 3), "номера идут подряд с 1, общие на всех авторов");
    assert_eq!(store.get(2).await.unwrap(), Some(second.clone()), "get возвращает опубликованный пост целиком");
    assert_eq!(store.get(99).await.unwrap(), None, "неизвестный номер даёт None");
    let mine = store.by_author(7).await.unwrap();
    assert_eq!(mine, vec![first, third], "посты автора идут в порядке публикации");
    assert!(store.by_author(5).await.unwrap().is_empty(), "у незнакомого автора постов нет");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn many_writers_get_unique_ids() {
    let (store, _runner) = Store::start(SlowDisk::new(0), Config { batch_max: 50, batch_wait: ms(5) });
    let mut tasks = Vec::new();
    for author in 0..20u32 {
        let store = store.clone();
        tasks.push(tokio::spawn(async move {
            let mut ids = Vec::new();
            for n in 0..10 {
                ids.push(store.publish(author, &format!("пост {n}")).await.unwrap().id);
            }
            ids
        }));
    }
    let mut all = Vec::new();
    for task in tasks {
        let ids = task.await.unwrap();
        assert!(ids.windows(2).all(|w| w[0] < w[1]), "номера одного автора возрастают");
        all.extend(ids);
    }
    all.sort();
    assert_eq!(all, (1..=200).collect::<Vec<u64>>(), "двести публикаций: номера от 1 до 200 без пропусков и повторов");
    let theirs = store.by_author(3).await.unwrap();
    assert_eq!(theirs.len(), 10, "у каждого автора по десять постов");
    assert!(theirs.windows(2).all(|w| w[0].id < w[1].id), "посты автора в порядке публикации");
}

#[tokio::test(start_paused = true)]
async fn subscribers_see_posts_published_after_they_joined() {
    let (store, _runner) = Store::start(SlowDisk::new(10), config());
    store.publish(1, "без подписчиков").await.expect("публикация без единого подписчика должна работать");
    let mut a = store.subscribe();
    let mut b = store.subscribe();
    for n in 0..4 {
        store.publish(1, &format!("пост {n}")).await.unwrap();
    }
    let mut late = store.subscribe();
    store.publish(2, "последний").await.unwrap();
    for (name, rx) in [("a", &mut a), ("b", &mut b)] {
        let mut ids = Vec::new();
        for _ in 0..5 {
            ids.push(rx.recv().await.unwrap().id);
        }
        assert_eq!(ids, vec![2, 3, 4, 5, 6], "подписчик {name} видит все посты после подписки по порядку");
    }
    assert_eq!(late.recv().await.unwrap().id, 6, "поздний подписчик видит только пост, вышедший после его подписки");
}

#[tokio::test(start_paused = true)]
async fn disk_gets_full_batches_then_the_remainder_on_time() {
    let disk = SlowDisk::new(10);
    let (store, _runner) = Store::start(disk.clone(), config());
    for n in 0..7 {
        store.publish(1, &format!("пост {n}")).await.unwrap();
    }
    sleep(ms(300)).await;
    let batches = disk.batches();
    let ids: Vec<Vec<u64>> = batches.iter().map(|(_, ids)| ids.clone()).collect();
    assert_eq!(ids, vec![vec![1, 2, 3], vec![4, 5, 6], vec![7]], "пачки по три, остаток отдельно, порядок публикации");
    assert!(batches[0].0 < ms(50), "полная пачка уходит сразу, а не по таймеру: ушла в {:?}", batches[0].0);
    assert!(batches[1].0 < ms(100), "вторая полная пачка тоже без ожидания: ушла в {:?}", batches[1].0);
    assert!(
        batches[2].0 >= ms(100) && batches[2].0 <= ms(150),
        "остаток из одного поста ждёт batch_wait (100 мс) от своего появления, ушёл в {:?}",
        batches[2].0
    );
}

#[tokio::test(start_paused = true)]
async fn a_slow_disk_does_not_slow_publishing_down() {
    let disk = SlowDisk::new(500);
    let (store, runner) = Store::start(disk.clone(), config());
    let start = Instant::now();
    for n in 0..10 {
        store.publish(1, &format!("пост {n}")).await.unwrap();
    }
    assert!(start.elapsed() < ms(100), "десять публикаций заняли {:?}: запись на диск тормозит публикацию", start.elapsed());
    drop(store);
    runner.finish().await;
    assert_eq!(disk.all_ids(), (1..=10).collect::<Vec<u64>>(), "после остановки на диске все десять постов, по порядку");
}

#[tokio::test(start_paused = true)]
async fn finish_writes_the_unfinished_batch_without_waiting() {
    let disk = SlowDisk::new(10);
    let (store, runner) = Store::start(disk.clone(), config());
    for n in 0..5 {
        store.publish(1, &format!("пост {n}")).await.unwrap();
    }
    drop(store);
    let start = Instant::now();
    runner.finish().await;
    assert_eq!(disk.all_ids(), vec![1, 2, 3, 4, 5], "неполная пачка [4, 5] должна быть записана при остановке, а не потеряна");
    assert!(start.elapsed() < ms(100), "остановка ждала batch_wait ({:?}), хотя новых постов уже не будет", start.elapsed());
}

#[tokio::test(start_paused = true)]
async fn an_idle_store_writes_nothing() {
    let disk = SlowDisk::new(10);
    let (store, runner) = Store::start(disk.clone(), config());
    sleep(ms(500)).await;
    drop(store);
    runner.finish().await;
    assert!(disk.batches().is_empty(), "постов не было, на диск ничего писать не нужно (пустых пачек не бывает)");
}
