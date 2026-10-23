// Кэш с необязательной статистикой и логированием. Список возможностей печатает example probe.
// Проблемы две: с feature stats библиотека не собирается, а feature no_log работает «наоборот»:
// её включение что-то отнимает. TODO: исправь и Cargo.toml, и cfg-атрибуты. Подробности в README.md.

#[cfg(feature = "stats")]
pub mod stats {
    use tinymap::TinyMap;

    pub fn hits() -> TinyMap<String, u32> {
        let mut map = TinyMap::new();
        map.bump("get".to_string());
        map.bump("get".to_string());
        map
    }
}

pub fn capabilities() -> Vec<&'static str> {
    let mut caps = vec!["get", "put"];
    #[cfg(not(feature = "no_log"))]
    caps.push("log");
    #[cfg(feature = "stats")]
    caps.push("stats");
    caps
}
