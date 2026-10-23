pub fn pause_from(value: Option<&str>) -> u64 {
    value.and_then(|v| v.parse().ok()).unwrap_or(100)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pause_defaults_to_100() {
        assert_eq!(pause_from(None), 100);
        assert_eq!(pause_from(Some("oops")), 100);
        assert_eq!(pause_from(Some("250")), 250);
    }
}
