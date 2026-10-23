// user-service перезапускается, и лента всех пользователей отвечает ошибкой. А ведь посты лежат в post-service целыми: не хватает
// только имён и аватарок авторов, которые час назад мы же и показывали. Лента с чуть устаревшими именами лучше пустой страницы
// с ошибкой. Это деградация: когда зависимость недоступна, отдаём то, что есть, и честно говорим, что это не свежее.
// Цель: профиль берётся по лестнице «свежий кэш, живой сервис, устаревший кэш в пределах срока годности, заглушка»; лента не падает из-за
// недоступного user-service, но помечается как деградировавшая. Настоящую ошибку «нет такого пользователя» глотать нельзя.


use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;
use tokio::time::{Instant, timeout};

fn main() {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub id: u64,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// Сервис не отвечает: можно деградировать.
    Unavailable,
    /// Такого пользователя нет: это ответ, а не авария.
    NotFound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Up,
    Down,
    /// Принимает запрос и не отвечает никогда.
    Hung,
}

/// Подмена user-service.
pub struct Users {
    names: Mutex<HashMap<u64, String>>,
    mode: Mutex<Mode>,
    calls: AtomicU32,
}

impl Users {
    pub fn new(names: &[(u64, &str)]) -> Users {
        Users {
            names: Mutex::new(names.iter().map(|(id, n)| (*id, n.to_string())).collect()),
            mode: Mutex::new(Mode::Up),
            calls: AtomicU32::new(0),
        }
    }

    pub fn set_mode(&self, mode: Mode) {
        *self.mode.lock().unwrap() = mode;
    }

    pub fn rename(&self, id: u64, name: &str) {
        self.names.lock().unwrap().insert(id, name.to_string());
    }

    pub fn calls(&self) -> u32 {
        self.calls.load(Ordering::SeqCst)
    }

    pub fn forget(&self, id: u64) {
        self.names.lock().unwrap().remove(&id);
    }

    pub async fn fetch(&self, id: u64) -> Result<Profile, Failure> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let mode = *self.mode.lock().unwrap();
        match mode {
            Mode::Down => Err(Failure::Unavailable),
            Mode::Hung => {
                tokio::time::sleep(Duration::from_secs(3600)).await;
                Err(Failure::Unavailable)
            }
            Mode::Up => self.names.lock().unwrap().get(&id).map(|name| Profile { id, name: name.clone() }).ok_or(Failure::NotFound),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    pub profile: Profile,
    /// Профиль взят из кэша после того, как перестал быть свежим.
    pub stale: bool,
    /// Профиля нет вообще, показана заглушка.
    pub placeholder: bool,
}

pub struct Profiles {
    users: Users,
    fresh_for: Duration,
    stale_for: Duration,
    call_timeout: Duration,
    cache: Mutex<HashMap<u64, (Profile, Instant)>>,
}

impl Profiles {
    /// Свежим профиль считается `fresh_for`; ещё `stale_for` после этого его можно показывать как устаревший.
    /// Ответа от сервиса ждём не дольше `call_timeout`.
    pub fn new(users: Users, fresh_for: Duration, stale_for: Duration, call_timeout: Duration) -> Profiles {
        Profiles { users, fresh_for, stale_for, call_timeout, cache: Mutex::new(HashMap::new()) }
    }

    pub fn users(&self) -> &Users {
        &self.users
    }

    pub async fn get(&self, id: u64) -> Result<Shown, Failure> {
        if let Some((profile, at)) = self.cache.lock().unwrap().get(&id) {
            if at.elapsed() < self.fresh_for {
                return Ok(Shown { profile: profile.clone(), stale: false, placeholder: false });
            }
        }
        let profile = self.users.fetch(id).await?;
        self.cache.lock().unwrap().insert(id, (profile.clone(), Instant::now()));
        let _ = (self.stale_for, self.call_timeout);
        Ok(Shown { profile, stale: false, placeholder: false })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Post {
    pub id: u64,
    pub author: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Feed {
    pub items: Vec<(Post, Shown)>,
    /// Хоть один профиль устаревший или заглушка: клиент покажет пометку.
    pub degraded: bool,
}

pub async fn load_feed(profiles: &Profiles, posts: &[Post]) -> Result<Feed, Failure> {
    let mut items = Vec::new();
    for post in posts {
        let shown = profiles.get(post.author).await?;
        items.push((post.clone(), shown));
    }
    let degraded = items.iter().any(|(_, s)| s.stale || s.placeholder);
    Ok(Feed { items, degraded })
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: fn(u64) -> Duration = Duration::from_millis;
    const FRESH: Duration = Duration::from_secs(30);
    const STALE: Duration = Duration::from_secs(600);

    fn profiles() -> Profiles {
        Profiles::new(Users::new(&[(1, "Аня"), (2, "Боря")]), FRESH, STALE, MS(200))
    }

    fn posts() -> Vec<Post> {
        vec![Post { id: 10, author: 1 }, Post { id: 11, author: 2 }, Post { id: 12, author: 1 }]
    }

    #[tokio::test(start_paused = true)]
    async fn a_fresh_profile_comes_from_the_cache() {
        let p = profiles();
        let first = p.get(1).await.unwrap();
        assert_eq!(first.profile.name, "Аня");
        tokio::time::advance(Duration::from_secs(20)).await;
        let second = p.get(1).await.unwrap();
        assert!(!second.stale);
        assert_eq!(p.users().calls(), 1, "в пределах срока свежести сервис не нужен");
    }

    #[tokio::test(start_paused = true)]
    async fn during_an_outage_a_stale_profile_is_shown_and_marked() {
        let p = profiles();
        p.get(1).await.unwrap();
        tokio::time::advance(Duration::from_secs(60)).await;
        p.users().set_mode(Mode::Down);
        let shown = p.get(1).await.expect("недоступный user-service не должен ронять запрос");
        assert_eq!(shown.profile.name, "Аня", "показываем то, что знаем");
        assert!(shown.stale, "и честно помечаем, что это устаревшее");
        assert!(!shown.placeholder);
    }

    #[tokio::test(start_paused = true)]
    async fn a_very_old_profile_is_replaced_by_a_placeholder() {
        let p = profiles();
        p.get(1).await.unwrap();
        tokio::time::advance(FRESH + STALE + MS(1)).await;
        p.users().set_mode(Mode::Down);
        let shown = p.get(1).await.unwrap();
        assert!(shown.placeholder, "срок годности устаревшего исчерпан: показывать десятичасовое имя нельзя");
        assert!(!shown.stale);
        assert_eq!(shown.profile.id, 1);
    }

    #[tokio::test(start_paused = true)]
    async fn an_unknown_user_during_an_outage_gets_a_placeholder() {
        let p = profiles();
        p.users().set_mode(Mode::Down);
        let shown = p.get(2).await.unwrap();
        assert!(shown.placeholder, "кэша нет, сервиса нет: заглушка");
    }

    #[tokio::test(start_paused = true)]
    async fn recovery_refreshes_the_profile() {
        let p = profiles();
        p.get(1).await.unwrap();
        tokio::time::advance(Duration::from_secs(60)).await;
        p.users().set_mode(Mode::Down);
        assert!(p.get(1).await.unwrap().stale);
        p.users().set_mode(Mode::Up);
        p.users().rename(1, "Анна");
        let shown = p.get(1).await.unwrap();
        assert_eq!(shown.profile.name, "Анна", "сервис вернулся, нужен свежий ответ");
        assert!(!shown.stale);
        let again = p.get(1).await.unwrap();
        assert!(!again.stale && again.profile.name == "Анна", "и он теперь лежит в кэше");
    }

    #[tokio::test(start_paused = true)]
    async fn a_real_not_found_is_not_masked() {
        let p = profiles();
        p.get(1).await.unwrap();
        tokio::time::advance(Duration::from_secs(60)).await;
        assert_eq!(p.get(99).await, Err(Failure::NotFound), "пользователя нет, это не авария, а ответ");
        p.users().forget(1);
        assert_eq!(p.get(1).await, Err(Failure::NotFound), "сервис уверенно сказал «нет такого»: кэшем это не перебить");
    }

    #[tokio::test(start_paused = true)]
    async fn a_hung_service_is_cut_by_the_call_timeout() {
        let p = profiles();
        p.get(1).await.unwrap();
        tokio::time::advance(Duration::from_secs(60)).await;
        p.users().set_mode(Mode::Hung);
        let start = Instant::now();
        let shown = p.get(1).await.unwrap();
        assert!(shown.stale);
        assert_eq!(start.elapsed(), MS(200), "зависший сервис нельзя ждать дольше таймаута вызова");
    }

    #[tokio::test(start_paused = true)]
    async fn the_feed_survives_an_outage_and_says_so() {
        let p = profiles();
        p.get(1).await.unwrap();
        tokio::time::advance(Duration::from_secs(60)).await;
        p.users().set_mode(Mode::Down);
        let feed = load_feed(&p, &posts()).await.expect("лента не должна падать из-за профилей");
        assert_eq!(feed.items.len(), 3);
        assert!(feed.degraded, "часть профилей устаревшая или заглушки: ленту надо пометить");
        assert!(feed.items[0].1.stale && feed.items[1].1.placeholder);
    }

    #[tokio::test(start_paused = true)]
    async fn a_healthy_feed_is_not_marked() {
        let p = profiles();
        let feed = load_feed(&p, &posts()).await.unwrap();
        assert!(!feed.degraded);
        assert_eq!(feed.items[1].1.profile.name, "Боря");
    }
}
