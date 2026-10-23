use echo_service::Settings;
use service_runtime::config::ConfigError;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::Duration;

fn real(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let map: HashMap<String, String> = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    move |name| map.get(name).cloned()
}

#[test]
fn settings_come_from_the_file_with_defaults() {
    let file = "# локальные настройки\nDATABASE_ADDR=127.0.0.1:5432\nAPI_TOKEN=\"s3cret # token\"\n";
    let settings = Settings::load(file, real(&[])).unwrap();
    assert_eq!(settings.port, 7000, "порт по умолчанию 7000");
    assert_eq!(settings.grace, Duration::from_millis(100), "grace по умолчанию 100 мс");
    assert_eq!(settings.drain_deadline, Duration::from_secs(2), "дренаж по умолчанию 2 секунды");
    assert_eq!(settings.database_addr, "127.0.0.1:5432", "адрес базы из файла");
    assert_eq!(settings.api_token.expose(), "s3cret # token", "токен в кавычках читается целиком");
}

#[test]
fn real_environment_beats_the_file() {
    let file = "PORT=1111\nDATABASE_ADDR=file:1\nAPI_TOKEN=t\n";
    let settings = Settings::load(file, real(&[("PORT", "2222"), ("DATABASE_ADDR", "env:2")])).unwrap();
    assert_eq!(settings.port, 2222, "PORT из окружения");
    assert_eq!(settings.database_addr, "env:2", "адрес из окружения");
}

#[test]
fn every_problem_is_reported_at_once() {
    let errors = Settings::load("PORT=abc\n", real(&[])).unwrap_err();
    assert_eq!(
        errors,
        vec![
            ConfigError::Invalid { name: "PORT".to_string(), value: "abc".to_string() },
            ConfigError::Missing("DATABASE_ADDR".to_string()),
            ConfigError::Missing("API_TOKEN".to_string()),
        ],
        "должны вернуться все три ошибки, в порядке полей"
    );
    let zero = Settings::load("PORT=0\nDATABASE_ADDR=x\nAPI_TOKEN=t\n", real(&[])).unwrap_err();
    assert_eq!(zero.len(), 1, "порт 0 недопустим");
}

#[test]
fn broken_env_file_is_a_config_error() {
    let errors = Settings::load("PORT=1\nэто не пара\n", real(&[])).unwrap_err();
    assert_eq!(errors.len(), 1, "кривая строка .env это одна ошибка конфигурации");
    assert!(errors[0].to_string().contains(".env"), "ошибка называет файл: {}", errors[0]);
}

#[test]
fn debug_output_hides_the_token() {
    let settings = Settings::load("DATABASE_ADDR=db:1\nAPI_TOKEN=hunter2\n", real(&[])).unwrap();
    let text = format!("{settings:?} {settings:#?}");
    assert!(!text.contains("hunter2"), "токен утёк в Debug конфига: {text}");
    assert!(text.contains("db:1"), "остальные поля печатаются: {text}");
}

#[test]
fn main_is_short_and_the_service_exposes_run() {
    let _ = || echo_service::run();
    let main = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs")).unwrap();
    assert!(main.lines().count() <= 25, "main.rs должен быть коротким: вся работа в lib.rs");
    assert!(!main.contains("env::var"), "main.rs не читает окружение сам: это делает Settings::load");
}
