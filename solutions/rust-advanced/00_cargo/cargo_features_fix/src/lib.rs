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
    #[cfg(feature = "log")]
    caps.push("log");
    #[cfg(feature = "stats")]
    caps.push("stats");
    caps
}
