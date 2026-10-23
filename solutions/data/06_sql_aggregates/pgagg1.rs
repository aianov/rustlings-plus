fn profile_fill() -> &'static str {
    "SELECT count(*) AS total, count(bio) AS with_bio FROM users"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn counts_filled_bios() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(profile_fill()).await;
        assert_eq!(t.columns, ["total", "with_bio"], "нужны столбцы total и with_bio");
        assert_eq!(t.rows, [["6", "3"]], "всего шесть пользователей, bio у троих:\n{t}");
    }
}
