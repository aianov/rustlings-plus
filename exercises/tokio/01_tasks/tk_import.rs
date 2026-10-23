// Массовый импорт пользователей. Строка "имя:пароль". Для каждой строки:
//   1. разобрать (нет двоеточия это ImportError::BadRecord(строка));
//   2. посчитать хеш пароля через `slow_hash`, это блокирующая работа, ей место в spawn_blocking;
//   3. «сохранить» (`save`, async).
// Условия: одновременно в работе не больше `limit` записей; результат в порядке входных строк;
// первая ошибка отменяет остальные записи (новые не стартуют, незавершённые не сохраняются);
// паника в задаче (slow_hash паникует на пароле "boom") превращается в ImportError::Task.
// TODO: Допиши `import` (и `process` по желанию). Всё остальное уже написано.
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::ThreadId;
use std::time::Duration;
use tokio::task::{JoinError, JoinSet};

#[derive(Debug, Clone, PartialEq)]
pub struct User {
    pub name: String,
    pub hash: String,
}

#[derive(Debug, PartialEq)]
pub enum ImportError {
    BadRecord(String),
    Task(String),
}

impl From<JoinError> for ImportError {
    fn from(error: JoinError) -> Self {
        ImportError::Task(error.to_string())
    }
}

#[derive(Default)]
pub struct Stats {
    pub hashing: AtomicUsize,
    pub peak_hashing: AtomicUsize,
    pub saved: AtomicUsize,
    pub hash_threads: Mutex<HashSet<ThreadId>>,
}

pub fn slow_hash(stats: &Stats, password: &str) -> String {
    let now = stats.hashing.fetch_add(1, Ordering::SeqCst) + 1;
    stats.peak_hashing.fetch_max(now, Ordering::SeqCst);
    stats.hash_threads.lock().unwrap().insert(std::thread::current().id());
    assert!(password != "boom", "хеширование упало");
    std::thread::sleep(Duration::from_millis(20));
    stats.hashing.fetch_sub(1, Ordering::SeqCst);
    format!("h{}", password.len() * 7)
}

pub async fn save(stats: &Stats, user: User) -> User {
    tokio::time::sleep(Duration::from_millis(5)).await;
    stats.saved.fetch_add(1, Ordering::SeqCst);
    user
}

pub async fn import(lines: Vec<String>, limit: usize, stats: Arc<Stats>) -> Result<Vec<User>, ImportError> {
    todo!()
}

#[tokio::main]
async fn main() {
    let stats = Arc::new(Stats::default());
    let lines = vec!["аня:secret".to_string(), "борис:qwerty".to_string()];
    println!("{:?}", import(lines, 2, stats).await);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(count: usize) -> Vec<String> {
        (0..count).map(|i| format!("user{i}:pass{i}")).collect()
    }

    #[tokio::test]
    async fn imports_in_input_order() {
        let stats = Arc::new(Stats::default());
        let users = import(lines(6), 3, Arc::clone(&stats)).await.unwrap();
        let names: Vec<_> = users.iter().map(|user| user.name.as_str()).collect();
        assert_eq!(names, ["user0", "user1", "user2", "user3", "user4", "user5"], "порядок результата как у входа");
        assert_eq!(users[0].hash, "h35", "хеш считается через slow_hash");
        assert_eq!(stats.saved.load(Ordering::SeqCst), 6, "каждая запись сохранена один раз");
    }

    #[tokio::test]
    async fn respects_the_limit_but_runs_in_parallel() {
        let stats = Arc::new(Stats::default());
        import(lines(12), 4, Arc::clone(&stats)).await.unwrap();
        let peak = stats.peak_hashing.load(Ordering::SeqCst);
        assert!(peak <= 4, "одновременно хешировалось {peak} записей при limit = 4");
        assert!(peak >= 3, "параллельности нет (пик {peak}): записи идут по одной или почти по одной");
    }

    #[tokio::test]
    async fn hashing_leaves_the_runtime_thread_alone() {
        let stats = Arc::new(Stats::default());
        import(lines(4), 2, Arc::clone(&stats)).await.unwrap();
        let here = std::thread::current().id();
        assert!(
            !stats.hash_threads.lock().unwrap().contains(&here),
            "хеш считался на потоке рантайма: тяжёлую часть нужно уносить в spawn_blocking"
        );
    }

    #[tokio::test]
    async fn bad_record_stops_everything() {
        let stats = Arc::new(Stats::default());
        let mut input = lines(10);
        input[3] = "без-двоеточия".to_string();
        let result = import(input, 2, Arc::clone(&stats)).await;
        assert_eq!(result, Err(ImportError::BadRecord("без-двоеточия".to_string())), "ошибка записи должна выйти наружу");
        tokio::time::sleep(Duration::from_millis(300)).await;
        let saved = stats.saved.load(Ordering::SeqCst);
        assert!(saved <= 4, "после ошибки сохранено {saved} записей из 10: остальные должны быть отменены");
    }

    #[tokio::test]
    async fn nothing_is_saved_after_the_error_is_returned() {
        let stats = Arc::new(Stats::default());
        let mut input = lines(10);
        input[2] = "плохая".to_string();
        let _ = import(input, 2, Arc::clone(&stats)).await;
        let at_return = stats.saved.load(Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(stats.saved.load(Ordering::SeqCst), at_return, "после возврата ошибки сохранения продолжаться не должны");
    }

    #[tokio::test]
    async fn panic_becomes_an_error() {
        let stats = Arc::new(Stats::default());
        let mut input = lines(5);
        input[1] = "злодей:boom".to_string();
        let result = import(input, 2, Arc::clone(&stats)).await;
        assert!(matches!(result, Err(ImportError::Task(_))), "паника в задаче должна стать ImportError::Task, а вышло {result:?}");
    }

    #[tokio::test]
    async fn empty_input() {
        let stats = Arc::new(Stats::default());
        assert_eq!(import(vec![], 3, stats).await, Ok(vec![]), "пустой вход даёт пустой список");
    }
}
