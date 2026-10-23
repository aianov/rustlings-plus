// Настройки читают постоянно и из многих задач, а меняют редко. Сейчас они лежат под обычным
// Mutex, и читатели (каждый держит настройки 100 мс, пока оформляет ответ) выстраиваются в очередь
// друг за другом, хотя читать одно и то же могут одновременно. Писатель при этом должен
// получать настройки в единоличное пользование.
// TODO: Замени замок на подходящий так, чтобы читатели шли вместе, а писатель по-прежнему ждал их всех.
use std::time::Duration;
use tokio::sync::Mutex;

struct Config {
    name: String,
    level: u32,
}

struct Settings {
    inner: Mutex<Config>,
}

impl Settings {
    fn new(name: &str, level: u32) -> Self {
        Settings { inner: Mutex::new(Config { name: name.to_string(), level }) }
    }

    async fn describe(&self) -> String {
        let config = self.inner.lock().await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        format!("{}:{}", config.name, config.level)
    }

    async fn set_level(&self, level: u32) {
        self.inner.lock().await.level = level;
    }
}

#[tokio::main]
async fn main() {
    let settings = Settings::new("app", 1);
    settings.set_level(2).await;
    println!("{}", settings.describe().await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::time::Instant;

    #[tokio::test(start_paused = true)]
    async fn readers_run_together() {
        let settings = Arc::new(Settings::new("app", 1));
        let started = Instant::now();
        let readers: Vec<_> = (0..5)
            .map(|_| {
                let settings = Arc::clone(&settings);
                tokio::spawn(async move { settings.describe().await })
            })
            .collect();
        for reader in readers {
            assert_eq!(reader.await.unwrap(), "app:1");
        }
        assert_eq!(
            started.elapsed(),
            Duration::from_millis(100),
            "пять читателей по 100 мс должны идти одновременно, а заняли {:?}",
            started.elapsed()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn writer_waits_for_readers_and_readers_see_the_new_value() {
        let settings = Arc::new(Settings::new("app", 1));
        let early = {
            let settings = Arc::clone(&settings);
            tokio::spawn(async move { settings.describe().await })
        };
        tokio::time::sleep(Duration::from_millis(10)).await;
        let started = Instant::now();
        settings.set_level(5).await;
        assert!(
            started.elapsed() >= Duration::from_millis(90),
            "писатель вклинился, пока читатель держал настройки: он должен дождаться читателей"
        );
        assert_eq!(early.await.unwrap(), "app:1", "читатель, начавший до записи, видел старое значение целиком");
        assert_eq!(settings.describe().await, "app:5", "после записи все видят новое");
    }
}
