// Страница профиля грузится из трёх мест: посты (база), аватар (кэш) и настройки (база). Ждать их
// надо вместе, а при первой же ошибке страницу уже не собрать, так что остальные запросы надо
// бросить, а не дожидаться: каждый стоит ресурсов. Запросы возвращают ошибки разных типов
// (DbError, CacheError), наружу выходит одна PageError.
// TODO: Почини `load_page`.
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::time::sleep;

#[derive(Debug, PartialEq)]
struct DbError(String);

#[derive(Debug, PartialEq)]
struct CacheError(String);

#[derive(Debug, PartialEq)]
enum PageError {
    Db(String),
    Cache(String),
}

impl From<DbError> for PageError {
    fn from(error: DbError) -> Self {
        PageError::Db(error.0)
    }
}

impl From<CacheError> for PageError {
    fn from(error: CacheError) -> Self {
        PageError::Cache(error.0)
    }
}

struct Env {
    posts_ms: u64,
    posts_fail: bool,
    avatar_ms: u64,
    avatar_fail: bool,
    settings_ms: u64,
    reached_end: AtomicUsize,
}

impl Env {
    fn new(posts_ms: u64, posts_fail: bool, avatar_ms: u64, avatar_fail: bool, settings_ms: u64) -> Env {
        Env { posts_ms, posts_fail, avatar_ms, avatar_fail, settings_ms, reached_end: AtomicUsize::new(0) }
    }

    async fn posts(&self) -> Result<Vec<u32>, DbError> {
        sleep(Duration::from_millis(self.posts_ms)).await;
        self.reached_end.fetch_add(1, Ordering::SeqCst);
        if self.posts_fail { Err(DbError("posts".into())) } else { Ok(vec![1, 2]) }
    }

    async fn avatar(&self) -> Result<String, CacheError> {
        sleep(Duration::from_millis(self.avatar_ms)).await;
        self.reached_end.fetch_add(1, Ordering::SeqCst);
        if self.avatar_fail { Err(CacheError("avatar".into())) } else { Ok("cat.png".into()) }
    }

    async fn settings(&self) -> Result<u8, DbError> {
        sleep(Duration::from_millis(self.settings_ms)).await;
        self.reached_end.fetch_add(1, Ordering::SeqCst);
        Ok(3)
    }
}

#[derive(Debug, PartialEq)]
struct Page {
    posts: Vec<u32>,
    avatar: String,
    theme: u8,
}

async fn load_page(env: &Env) -> Result<Page, PageError> {
    let posts = env.posts().await?;
    let avatar = env.avatar().await?;
    let theme = env.settings().await?;
    Ok(Page { posts, avatar, theme })
}

#[tokio::main]
async fn main() {
    let env = Env::new(50, false, 200, false, 100);
    println!("{:?}", load_page(&env).await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Instant;

    #[tokio::test(start_paused = true)]
    async fn all_ok_takes_the_slowest() {
        let env = Env::new(50, false, 200, false, 100);
        let start = Instant::now();
        let page = load_page(&env).await;
        assert_eq!(page, Ok(Page { posts: vec![1, 2], avatar: "cat.png".into(), theme: 3 }), "все три запроса успешны, страница должна собраться");
        assert_eq!(start.elapsed(), Duration::from_millis(200), "запросы идут вместе: ждём самый долгий (200 мс)");
    }

    #[tokio::test(start_paused = true)]
    async fn first_error_ends_the_wait() {
        let env = Env::new(50, true, 500, false, 200);
        let start = Instant::now();
        let page = load_page(&env).await;
        assert_eq!(page, Err(PageError::Db("posts".into())), "ошибка базы должна выйти наружу как PageError::Db");
        assert_eq!(start.elapsed(), Duration::from_millis(50), "после первой ошибки ждать остальных нельзя");
        sleep(Duration::from_secs(1)).await;
        assert_eq!(
            env.reached_end.load(Ordering::SeqCst),
            1,
            "остальные запросы должны быть брошены, а не дорабатывать в фоне"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn the_earliest_error_wins() {
        let env = Env::new(50, true, 30, true, 200);
        let start = Instant::now();
        let page = load_page(&env).await;
        assert_eq!(page, Err(PageError::Cache("avatar".into())), "раньше упал кэш (30 мс), его ошибка и нужна");
        assert_eq!(start.elapsed(), Duration::from_millis(30), "ждём только до первой ошибки");
    }
}
