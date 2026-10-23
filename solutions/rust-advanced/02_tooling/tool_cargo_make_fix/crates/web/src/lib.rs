pub fn title(page: &str) -> String {
    format!("Site: {page}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_titles() {
        assert_eq!(title("home"), "Site: home");
    }
}
