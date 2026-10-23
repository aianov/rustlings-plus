fn setting(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| "unset".to_string())
}

fn main() {
    println!("{} {}", setting("APP_NAME"), setting("LOG_LEVEL"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_setting_is_reported() {
        assert_eq!(setting("SURELY_NOT_DEFINED_ANYWHERE"), "unset");
    }
}
