// Конфиг честно читается из источника, и тесты подают в него свои значения. Но тест ниже
// собирает два приложения с разными настройками, а они ведут себя одинаково. Где-то в глубине
// кода остался прямой вызов `std::env::var`, который источник обходит. А в настоящем окружении
// тесты друг другу мешают: оно одно на весь процесс.
// TODO: Найди обход источника и проведи значение через `Config`.

use std::time::Duration;

#[derive(Debug, PartialEq)]
pub struct Config {
    pub port: u16,
    pub timeout: Duration,
}

impl Config {
    pub fn from_source(get: impl Fn(&str) -> Option<String>) -> Config {
        let port = get("PORT").and_then(|v| v.parse().ok()).unwrap_or(8080);
        let timeout = get("TIMEOUT_MS").and_then(|v| v.parse().ok()).map(Duration::from_millis);
        Config { port, timeout: timeout.unwrap_or(Duration::from_secs(5)) }
    }
}

fn retry_limit_from_env() -> u32 {
    std::env::var("RETRY_LIMIT").ok().and_then(|v| v.parse().ok()).unwrap_or(3)
}

pub struct App {
    config: Config,
    retry_limit: u32,
}

impl App {
    pub fn new(config: Config) -> App {
        App { config, retry_limit: retry_limit_from_env() }
    }

    pub fn port(&self) -> u16 {
        self.config.port
    }

    pub fn timeout(&self) -> Duration {
        self.config.timeout
    }

    pub fn retry_limit(&self) -> u32 {
        self.retry_limit
    }
}

fn main() {
    let app = App::new(Config::from_source(|n| std::env::var(n).ok()));
    println!("{} {}", app.port(), app.retry_limit());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn app_with(pairs: &[(&str, &str)]) -> App {
        let map: HashMap<String, String> =
            pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        App::new(Config::from_source(move |name| map.get(name).cloned()))
    }

    #[test]
    fn source_values_reach_the_app() {
        let app = app_with(&[("PORT", "9000"), ("RETRY_LIMIT", "7")]);
        assert_eq!(app.port(), 9000, "порт из источника");
        assert_eq!(app.retry_limit(), 7, "лимит повторов должен прийти из источника, а не из окружения процесса");
    }

    #[test]
    fn defaults_without_touching_the_environment() {
        let app = app_with(&[]);
        assert_eq!((app.port(), app.retry_limit()), (8080, 3), "умолчания: порт 8080 и три повтора");
        assert_eq!(app.timeout(), Duration::from_secs(5), "таймаут по умолчанию 5 секунд");
    }

    #[test]
    fn two_apps_at_once_keep_their_own_settings() {
        let (a, b) = std::thread::scope(|scope| {
            let spawn = |limit: &'static str| {
                std::thread::Builder::new()
                    .spawn_scoped(scope, move || app_with(&[("RETRY_LIMIT", limit)]).retry_limit())
                    .unwrap()
            };
            let a = spawn("1");
            let b = spawn("9");
            (a.join().unwrap(), b.join().unwrap())
        });
        assert_eq!((a, b), (1, 9), "два приложения в одном процессе не должны делить настройки");
    }
}
