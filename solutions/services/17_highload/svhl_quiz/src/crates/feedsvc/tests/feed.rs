use feedsvc::{Config, FeedError, FeedPage, FeedService, Users};
use futures::future::join_all;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::Instant;

const MS: fn(u64) -> Duration = Duration::from_millis;

fn users(n: u64, latency: Duration) -> Arc<Users> {
    Arc::new(Users::new((1..=n).map(|i| (i, format!("автор{i}"))).collect(), latency))
}

fn names_of(page: &FeedPage) -> Vec<(u64, String)> {
    page.items.iter().map(|i| (i.post.id, i.author_name.clone())).collect()
}

async fn walk(svc: &FeedService, user: u64, limit: usize) -> Vec<(u64, String)> {
    let mut all = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..10_000 {
        let page = svc.feed(user, cursor.as_deref(), limit).await.unwrap();
        all.extend(names_of(&page));
        match page.next {
            Some(c) => cursor = Some(c),
            None => return all,
        }
    }
    panic!("обход ленты не заканчивается");
}

struct Lcg(u64);
impl Lcg {
    fn below(&mut self, n: u64) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 33) % n
    }
}

/// Простая модель: все посты в одном списке, лента считается обходом всего.
#[derive(Default)]
struct Model {
    posts: Vec<(u64, u64)>,
    follows: HashMap<u64, Vec<u64>>,
}

impl Model {
    fn feed(&self, user: u64) -> Vec<(u64, String)> {
        let followed = self.follows.get(&user).cloned().unwrap_or_default();
        let mut mine: Vec<(u64, u64)> = self.posts.iter().copied().filter(|(_, a)| followed.contains(a)).collect();
        mine.sort_by(|a, b| b.0.cmp(&a.0));
        mine.into_iter().map(|(id, a)| (id, format!("автор{a}"))).collect()
    }
}

#[tokio::test(start_paused = true)]
async fn the_service_agrees_with_a_simple_model() {
    let svc = FeedService::new(users(20, MS(5)), Config::default());
    let mut model = Model::default();
    let mut rng = Lcg(2024);
    for _ in 0..120 {
        let (user, author) = (1 + rng.below(30), 1 + rng.below(20));
        if user != author {
            svc.follow(user, author);
            let list = model.follows.entry(user).or_default();
            if !list.contains(&author) {
                list.push(author);
            }
        }
    }
    for round in 0..600 {
        if rng.below(3) < 2 {
            let author = 1 + rng.below(20);
            let id = svc.publish(author, &format!("пост {round}"));
            model.posts.push((id, author));
            tokio::time::sleep(MS(rng.below(3))).await;
        } else {
            let user = 1 + rng.below(30);
            let limit = 1 + rng.below(8) as usize;
            let got = walk(&svc, user, limit).await;
            assert_eq!(got, model.feed(user), "лента пользователя {user} (страницы по {limit}) расходится с моделью на шаге {round}");
        }
    }
}

#[tokio::test(start_paused = true)]
async fn a_celebrity_post_is_not_written_into_every_timeline() {
    let svc = FeedService::new(users(10, MS(5)), Config::default());
    for fan in 100..160 {
        svc.follow(fan, 1);
    }
    svc.follow(100, 2);
    let id = svc.publish(1, "для всех");
    assert_eq!(svc.line_writes(), 0, "знаменитость с 60 подписчиками (порог 5) разложена по лентам: {} записей", svc.line_writes());
    let page = svc.feed(130, None, 10).await.unwrap();
    assert_eq!(names_of(&page), vec![(id, "автор1".to_string())], "подписчик видит пост знаменитости сразу");
    svc.publish(2, "обычный");
    assert_eq!(svc.line_writes(), 1, "у автора 2 один подписчик: одна запись в ленту");
}

#[tokio::test(start_paused = true)]
async fn timelines_are_capped() {
    let cfg = Config { cap: 5, ..Config::default() };
    let svc = FeedService::new(users(5, MS(5)), cfg);
    svc.follow(50, 1);
    let ids: Vec<u64> = (0..12).map(|i| svc.publish(1, &format!("п{i}"))).collect();
    let got: Vec<u64> = walk(&svc, 50, 3).await.into_iter().map(|(id, _)| id).collect();
    assert_eq!(got, ids.iter().rev().take(5).copied().collect::<Vec<_>>(), "лента хранит только cap = 5 последних постов");
}

#[tokio::test(start_paused = true)]
async fn author_names_come_in_one_batch_and_are_cached() {
    let u = users(8, MS(20));
    let svc = FeedService::new(u.clone(), Config::default());
    svc.follow(50, 1);
    for author in 2..=8 {
        svc.follow(50, author);
    }
    for i in 0..40 {
        svc.publish(1 + i % 8, "текст");
    }
    let page = svc.feed(50, None, 40).await.unwrap();
    assert_eq!(page.items.len(), 40);
    assert_eq!((u.batch_calls(), u.ids_asked()), (1, 8), "40 постов восьми авторов: один вызов на восемь идентификаторов");
    svc.feed(50, None, 40).await.unwrap();
    assert_eq!(u.batch_calls(), 1, "второй показ ленты берёт имена из кэша");
}

#[tokio::test(start_paused = true)]
async fn a_crowd_of_identical_cold_requests_makes_one_call() {
    let u = users(8, MS(20));
    let svc = Arc::new(FeedService::new(u.clone(), Config::default()));
    for author in 1..=8 {
        svc.follow(50, author);
        svc.publish(author, "текст");
    }
    let start = Instant::now();
    let pages = join_all((0..30).map(|_| {
        let svc = svc.clone();
        async move { svc.feed(50, None, 10).await.unwrap() }
    }))
    .await;
    assert!(pages.iter().all(|p| p.items.len() == 8));
    assert_eq!(u.batch_calls(), 1, "тридцать одновременных запросов вызвали user-service {} раз", u.batch_calls());
    assert_eq!(start.elapsed(), MS(20));
}

#[tokio::test(start_paused = true)]
async fn invalidation_by_event_refreshes_only_that_profile() {
    let u = users(4, MS(5));
    let svc = FeedService::new(u.clone(), Config::default());
    for author in 1..=4 {
        svc.follow(50, author);
        svc.publish(author, "текст");
    }
    let first = svc.feed(50, None, 10).await.unwrap();
    assert!(names_of(&first).iter().any(|(_, n)| n == "автор3"));
    let asked = u.ids_asked();
    u.rename(3, "Новое имя");
    let cached = svc.feed(50, None, 10).await.unwrap();
    assert!(names_of(&cached).iter().any(|(_, n)| n == "автор3"), "до события кэш показывает прежнее имя");
    svc.on_profile_updated(3);
    let fresh = svc.feed(50, None, 10).await.unwrap();
    assert!(names_of(&fresh).iter().any(|(_, n)| n == "Новое имя"), "после события имя должно обновиться");
    assert_eq!(u.ids_asked(), asked + 1, "запрошен только изменившийся профиль");
}

#[tokio::test(start_paused = true)]
async fn profiles_expire_with_a_spread_and_the_cache_is_bounded() {
    let cfg = Config { profile_ttl: Duration::from_secs(100), profile_capacity: 4, ..Config::default() };
    let u = users(8, MS(5));
    let svc = FeedService::new(u.clone(), cfg);
    for author in 1..=8 {
        svc.follow(50, author);
        svc.publish(author, "текст");
    }
    svc.feed(50, None, 10).await.unwrap();
    assert!(svc.profiles().cached() <= 4, "в кэше {} записей при ёмкости 4", svc.profiles().cached());
    let cfg = Config { profile_ttl: Duration::from_secs(100), ..Config::default() };
    let svc = FeedService::new(u.clone(), cfg);
    for author in 1..=8 {
        svc.follow(50, author);
        svc.publish(author, "текст");
    }
    svc.feed(50, None, 10).await.unwrap();
    let calls = u.batch_calls();
    tokio::time::advance(Duration::from_secs(89)).await;
    svc.feed(50, None, 10).await.unwrap();
    assert_eq!(u.batch_calls(), calls, "за 89 секунд из 100 ещё ничего не должно было протухнуть");
    tokio::time::advance(Duration::from_secs(23)).await;
    svc.feed(50, None, 10).await.unwrap();
    assert_eq!(u.batch_calls(), calls + 1, "через 112 секунд все записи просрочены и берутся заново");
}

#[tokio::test(start_paused = true)]
async fn a_broken_cursor_is_an_error_and_unknown_authors_are_marked() {
    let svc = FeedService::new(users(2, MS(5)), Config::default());
    svc.follow(50, 1);
    svc.follow(50, 77);
    svc.publish(1, "а");
    svc.publish(77, "б");
    assert_eq!(svc.feed(50, Some("нет"), 10).await.unwrap_err(), FeedError::BadCursor);
    let page = svc.feed(50, None, 10).await.unwrap();
    let got: Vec<&str> = page.items.iter().map(|i| i.author_name.as_str()).collect();
    assert_eq!(got, vec!["?", "автор1"]);
}

#[tokio::test(start_paused = true)]
async fn latency_stays_near_the_dependency_latency_under_load() {
    let u = users(20, MS(20));
    let svc = Arc::new(FeedService::new(u.clone(), Config::default()));
    for user in 100..164 {
        for author in 1..=20 {
            svc.follow(user, author);
        }
    }
    for author in 1..=20 {
        svc.publish(author, "текст");
    }
    let mut times: Vec<Duration> = join_all((100..164).map(|user| {
        let svc = svc.clone();
        async move {
            let start = Instant::now();
            svc.feed(user, None, 20).await.unwrap();
            start.elapsed()
        }
    }))
    .await;
    times.sort();
    let p99 = times[times.len() * 99 / 100];
    assert!(p99 <= MS(45), "p99 = {p99:?} при задержке user-service 20 мс: одинаковые промахи должны делить один вызов");
    assert!(u.batch_calls() <= 3, "64 параллельных ленты вызвали user-service {} раз", u.batch_calls());
}
