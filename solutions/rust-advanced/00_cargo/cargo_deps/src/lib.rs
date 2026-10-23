pub const VERSION: &str = env!("REPORT_VERSION");

pub fn render(rows: &[(&str, u32)]) -> String {
    fmtkit::table(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_sample_rows() {
        let rows = testkit::sample_rows();
        assert_eq!(render(&rows), "apples | 3\nfigs   | 12");
    }
}
