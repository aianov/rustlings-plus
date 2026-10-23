pub fn format_plain(level: &str, msg: &str) -> String {
    format!("[{level}] {msg}")
}

#[cfg(feature = "json")]
pub fn format_json(level: &str, msg: &str) -> String {
    let escaped = msg.replace('\\', "\\\\").replace('"', "\\\"");
    format!("{{\"level\":\"{level}\",\"msg\":\"{escaped}\"}}")
}

#[cfg(feature = "color")]
pub fn format_color(level: &str, msg: &str) -> String {
    let line = format_plain(level, msg);
    if level == "ERROR" { colorize::red(&line) } else { line }
}

pub fn enabled_features() -> Vec<&'static str> {
    let mut list = Vec::new();
    if cfg!(feature = "json") {
        list.push("json");
    }
    if cfg!(feature = "color") {
        list.push("color");
    }
    list
}
