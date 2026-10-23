use std::time::Duration;

#[derive(Debug, PartialEq)]
pub struct Config {
    pub port: u16,
    pub timeout: Duration,
    pub retry_limit: u32,
}

impl Config {
    pub fn from_source(get: impl Fn(&str) -> Option<String>) -> Config {
        let port = get("PORT").and_then(|v| v.parse().ok()).unwrap_or(8080);
        let timeout = get("TIMEOUT_MS").and_then(|v| v.parse().ok()).map(Duration::from_millis);
        let retry_limit = get("RETRY_LIMIT").and_then(|v| v.parse().ok()).unwrap_or(3);
        Config { port, timeout: timeout.unwrap_or(Duration::from_secs(5)), retry_limit }
    }
}

pub struct App {
    config: Config,
}

impl App {
    pub fn new(config: Config) -> App {
        App { config }
    }

    pub fn port(&self) -> u16 {
        self.config.port
    }

    pub fn timeout(&self) -> Duration {
        self.config.timeout
    }

    pub fn retry_limit(&self) -> u32 {
        self.config.retry_limit
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
