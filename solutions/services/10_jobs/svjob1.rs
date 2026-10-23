use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::task::JoinHandle;

fn main() {}

#[derive(Default)]
pub struct Mailer {
    sent: Mutex<Vec<String>>,
    failures: Mutex<u32>,
    broken: Mutex<bool>,
}

impl Mailer {
    pub fn set_broken(&self, broken: bool) {
        *self.broken.lock().unwrap() = broken;
    }

    pub async fn send(&self, to: &str) -> Result<(), String> {
        tokio::time::sleep(Duration::from_millis(200)).await;
        if *self.broken.lock().unwrap() {
            *self.failures.lock().unwrap() += 1;
            return Err("почтовик не отвечает".to_string());
        }
        self.sent.lock().unwrap().push(to.to_string());
        Ok(())
    }

    pub fn sent(&self) -> Vec<String> {
        self.sent.lock().unwrap().clone()
    }

    pub fn failures(&self) -> u32 {
        *self.failures.lock().unwrap()
    }
}

#[derive(Default)]
pub struct Db {
    users: Mutex<Vec<String>>,
}

impl Db {
    pub fn insert(&self, name: &str) -> u64 {
        let mut users = self.users.lock().unwrap();
        users.push(name.to_string());
        users.len() as u64
    }

    pub fn count(&self) -> usize {
        self.users.lock().unwrap().len()
    }
}

#[derive(Default)]
pub struct Background {
    tasks: Mutex<Vec<JoinHandle<()>>>,
}

impl Background {
    pub fn spawn(&self, work: impl Future<Output = ()> + Send + 'static) {
        self.tasks.lock().unwrap().push(tokio::spawn(work));
    }

    pub async fn drain(&self) {
        let tasks = std::mem::take(&mut *self.tasks.lock().unwrap());
        for task in tasks {
            let _ = task.await;
        }
    }
}

pub async fn signup(db: &Db, mailer: &Arc<Mailer>, background: &Background, name: &str, email: &str) -> u64 {
    let id = db.insert(name);
    let mailer = Arc::clone(mailer);
    let email = email.to_string();
    background.spawn(async move {
        let _ = mailer.send(&email).await;
    });
    id
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Instant;

    #[tokio::test(start_paused = true)]
    async fn signup_answers_at_once_and_the_letter_goes_in_the_background() {
        let (db, mailer, background) = (Db::default(), Arc::new(Mailer::default()), Background::default());
        let started = Instant::now();
        let id = signup(&db, &mailer, &background, "alice", "alice@example.com").await;
        assert_eq!(id, 1, "номер новой записи");
        assert!(started.elapsed() < Duration::from_millis(50), "ответ ждал почтовик: прошло {:?}, а должно быть почти ноль", started.elapsed());
        assert!(mailer.sent().is_empty(), "письмо ещё не ушло: почтовику нужно 200 мс");
        background.drain().await;
        assert_eq!(mailer.sent(), vec!["alice@example.com"], "после drain письмо отправлено");
        assert!(started.elapsed() >= Duration::from_millis(200), "письмо реально ждало почтовика");
    }

    #[tokio::test(start_paused = true)]
    async fn a_broken_mailer_does_not_break_the_signup() {
        let (db, mailer, background) = (Db::default(), Arc::new(Mailer::default()), Background::default());
        mailer.set_broken(true);
        let id = signup(&db, &mailer, &background, "bob", "bob@example.com").await;
        assert_eq!(id, 1, "регистрация прошла, хотя почтовик лежит");
        background.drain().await;
        assert_eq!(db.count(), 1, "пользователь сохранён");
        assert_eq!(mailer.failures(), 1, "одна попытка отправки была, и она не удалась");
    }

    #[tokio::test(start_paused = true)]
    async fn many_signups_do_not_queue_behind_each_other() {
        let (db, mailer, background) = (Db::default(), Arc::new(Mailer::default()), Background::default());
        let started = Instant::now();
        for i in 0..10 {
            signup(&db, &mailer, &background, &format!("user{i}"), &format!("user{i}@example.com")).await;
        }
        assert!(started.elapsed() < Duration::from_millis(50), "десять регистраций подряд, и ни одна не ждала почтовика");
        background.drain().await;
        assert_eq!(mailer.sent().len(), 10, "все десять писем ушли");
        assert!(started.elapsed() < Duration::from_millis(400), "письма идут параллельно: за {:?} последовательно было бы 2 секунды", started.elapsed());
    }
}
