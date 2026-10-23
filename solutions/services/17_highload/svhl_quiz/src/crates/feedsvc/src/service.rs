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
        let now = Instant::now();
        let mut out = HashMap::new();
        let mut missing = Vec::new();
        {
            let mut cache = self.cache.lock().unwrap();
            for &id in ids {
                match cache.get(id) {
                    Some((name, expires)) if now < *expires => {
                        out.insert(id, name.clone());
                    }
                    _ => missing.push(id),
                }
            }
        }
        missing.sort_unstable();
        missing.dedup();
        if missing.is_empty() {
            return out;
        }
        let key = missing.iter().map(u64::to_string).collect::<Vec<_>>().join(",");
        let users = self.users.clone();
        let wanted = missing.clone();
        let fetched = self
            .flight
            .run(&key, || async move {
                let mut all = HashMap::new();
                for chunk in wanted.chunks(100) {
                    all.extend(users.get_many(chunk).await);
                }
                Arc::new(all)
            })
            .await;
        let mut cache = self.cache.lock().unwrap();
        for (id, name) in fetched.iter() {
            cache.put(*id, (name.clone(), Instant::now() + ttl_for(self.ttl, *id)));
            out.insert(*id, name.clone());
        }
        out
    }

    /// Забывает имя (профиль изменился).
    pub fn invalidate(&self, id: u64) {
        self.cache.lock().unwrap().remove(id);
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
    post_authors: HashMap<u64, u64>,
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
        let id = self.ids.lock().unwrap().generate();
        let post = Post { id, created_ms: id >> 22, text: text.to_string() };
        let _ = author;
        let mut state = self.state.lock().unwrap();
        state.posts.insert(id, post);
        state.authors_mut().insert(id, author);
        let followers = state.followers.get(&author).cloned().unwrap_or_default();
        if followers.len() >= self.cfg.celebrity_threshold {
            let store = state.celebrity.entry(author).or_default();
            store.push_front(id);
            store.truncate(self.cfg.cap);
        } else {
            for user in followers {
                let line = state.lines.entry(user).or_default();
                line.push_front(id);
                line.truncate(self.cfg.cap);
                state.line_writes += 1;
            }
        }
        id
    }

    /// Страница ленты пользователя: от новых к старым, строго старше курсора, не больше `limit` (от 1 до 50), у каждого поста имя автора.
    /// Личная лента и свежие посты знаменитостей, на которых подписан пользователь, сливаются по номеру. Курсор следующей
    /// страницы есть, только если дальше что-то осталось.
    pub async fn feed(&self, user: u64, cursor: Option<&str>, limit: usize) -> Result<FeedPage, FeedError> {
        let after = cursor.map(decode_cursor).transpose().map_err(|_| FeedError::BadCursor)?;
        let limit = limit.clamp(1, 50);
        let (page, has_more, authors): (Vec<Post>, bool, Vec<(u64, u64)>) = {
            let state = self.state.lock().unwrap();
            let mut ids: HashSet<u64> = state.lines.get(&user).map(|l| l.iter().copied().collect()).unwrap_or_default();
            for author in state.following.get(&user).map(Vec::as_slice).unwrap_or(&[]) {
                if let Some(store) = state.celebrity.get(author) {
                    ids.extend(store.iter().copied());
                }
            }
            let mut ids: Vec<u64> = ids.into_iter().filter(|id| after.is_none_or(|(_, last)| *id < last)).collect();
            ids.sort_unstable_by(|a, b| b.cmp(a));
            let has_more = ids.len() > limit;
            ids.truncate(limit);
            let posts: Vec<Post> = ids.iter().map(|id| state.posts[id].clone()).collect();
            let authors = ids.iter().map(|id| (*id, state.author_of(*id))).collect();
            (posts, has_more, authors)
        };
        let unique: Vec<u64> = {
            let mut seen = HashSet::new();
            authors.iter().map(|(_, a)| *a).filter(|a| seen.insert(*a)).collect()
        };
        let names = self.profiles.names(&unique).await;
        let items: Vec<FeedItem> = page
            .into_iter()
            .zip(authors)
            .map(|(post, (_, author))| FeedItem { author_name: names.get(&author).cloned().unwrap_or_else(|| "?".to_string()), post })
            .collect();
        let next = if has_more { items.last().map(|i| encode_cursor(i.post.created_ms, i.post.id)) } else { None };
        Ok(FeedPage { items, next })
    }

    /// Профиль изменился (событие `profile.updated`).
    pub fn on_profile_updated(&self, user: u64) {
        self.profiles.invalidate(user);
    }
}

impl State {
    fn authors_mut(&mut self) -> &mut HashMap<u64, u64> {
        &mut self.post_authors
    }

    fn author_of(&self, post: u64) -> u64 {
        self.post_authors[&post]
    }
}
