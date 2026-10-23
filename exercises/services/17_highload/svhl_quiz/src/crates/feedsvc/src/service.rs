use crate::cursor::{Post, decode_cursor, encode_cursor};
use crate::flight::SingleFlight;
use crate::ids::IdGen;
use crate::lru::{Lru, ttl_for};
use crate::users::Users;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::Instant;

#[derive(Debug, Clone)]
pub struct Config {
    /// Длина личной ленты и хранилища знаменитости.
    pub cap: usize,
    /// С такого числа подписчиков автор считается знаменитостью.
    pub celebrity_threshold: usize,
    pub profile_ttl: Duration,
    pub profile_capacity: usize,
}

impl Default for Config {
    fn default() -> Config {
        Config { cap: 1000, celebrity_threshold: 5, profile_ttl: Duration::from_secs(60), profile_capacity: 1000 }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum FeedError {
    BadCursor,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedItem {
    pub post: Post,
    pub author_name: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct FeedPage {
    pub items: Vec<FeedItem>,
    pub next: Option<String>,
}

/// Кэш имён авторов: LRU с разбросом срока жизни, одновременные одинаковые промахи делят один вызов в user-service.
pub struct Profiles {
    users: Arc<Users>,
    ttl: Duration,
    cache: Mutex<Lru<(String, Instant)>>,
    flight: SingleFlight<Arc<HashMap<u64, String>>>,
}

impl Profiles {
    pub fn new(users: Arc<Users>, ttl: Duration, capacity: usize) -> Profiles {
        Profiles { users, ttl, cache: Mutex::new(Lru::new(capacity)), flight: SingleFlight::default() }
    }

    pub fn cached(&self) -> usize {
        self.cache.lock().unwrap().len()
    }

    /// Имена по идентификаторам. Живые записи берутся из кэша, всё недостающее запрашивается ОДНИМ списком (по 100 за вызов),
    /// одновременные запросы одного и того же недостающего набора делят один вызов. Полученное кладётся в кэш с `ttl_for`.
    /// Неизвестные пользователи в ответ не попадают.
    pub async fn names(&self, ids: &[u64]) -> HashMap<u64, String> {
        let _ = (ids, &self.users, self.ttl, &self.cache, &self.flight, ttl_for(self.ttl, 0));
        todo!()
    }

    /// Забывает имя (профиль изменился).
    pub fn invalidate(&self, id: u64) {
        let _ = id;
        todo!()
    }
}

#[derive(Default)]
struct State {
    posts: HashMap<u64, Post>,
    followers: HashMap<u64, Vec<u64>>,
    following: HashMap<u64, Vec<u64>>,
    lines: HashMap<u64, VecDeque<u64>>,
    celebrity: HashMap<u64, VecDeque<u64>>,
    line_writes: u64,
}

pub struct FeedService {
    cfg: Config,
    ids: Mutex<IdGen<Box<dyn Fn() -> u64 + Send>>>,
    state: Mutex<State>,
    profiles: Profiles,
}

impl FeedService {
    pub fn new(users: Arc<Users>, cfg: Config) -> FeedService {
        let started = Instant::now();
        let clock: Box<dyn Fn() -> u64 + Send> = Box::new(move || started.elapsed().as_millis() as u64 + 1);
        let profiles = Profiles::new(users, cfg.profile_ttl, cfg.profile_capacity);
        FeedService { cfg, ids: Mutex::new(IdGen::new(1, clock)), state: Mutex::new(State::default()), profiles }
    }

    pub fn profiles(&self) -> &Profiles {
        &self.profiles
    }

    /// Сколько раз всего дописывали в личные ленты.
    pub fn line_writes(&self) -> u64 {
        self.state.lock().unwrap().line_writes
    }

    pub fn follow(&self, user: u64, author: u64) {
        let mut state = self.state.lock().unwrap();
        let list = state.followers.entry(author).or_default();
        if !list.contains(&user) {
            list.push(user);
            state.following.entry(user).or_default().push(author);
        }
    }

    /// Публикует пост и возвращает его номер. Автор, у которого подписчиков не меньше порога, считается знаменитостью:
    /// его пост кладётся один раз в общее хранилище (последние `cap`), остальным авторам пост раскладывается по личным лентам
    /// подписчиков (в каждой не больше `cap` последних).
    pub fn publish(&self, author: u64, text: &str) -> u64 {
        let _ = (author, text, &self.cfg, &self.ids, &self.state);
        todo!()
    }

    /// Страница ленты пользователя: от новых к старым, строго старше курсора, не больше `limit` (от 1 до 50), у каждого поста имя автора.
    /// Личная лента и свежие посты знаменитостей, на которых подписан пользователь, сливаются по номеру. Курсор следующей
    /// страницы есть, только если дальше что-то осталось.
    pub async fn feed(&self, user: u64, cursor: Option<&str>, limit: usize) -> Result<FeedPage, FeedError> {
        let _ = (user, cursor, limit, decode_cursor, encode_cursor, HashSet::<u64>::new());
        todo!()
    }

    /// Профиль изменился (событие `profile.updated`).
    pub fn on_profile_updated(&self, user: u64) {
        self.profiles.invalidate(user);
    }
}
