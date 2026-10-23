fn countries() -> &'static str {
    "SELECT DISTINCT country FROM users"
}

fn popular_authors() -> &'static str {
    "SELECT DISTINCT author_id FROM posts WHERE likes >= 12"
}

fn country_status_pairs() -> &'static str {
    "SELECT DISTINCT country, status FROM users"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    fn set(mut v: Vec<&str>) -> Vec<&str> {
        v.sort();
        v
    }

    #[tokio::test]
    async fn one_column() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(countries()).await;
        assert_eq!(set(t.column(0)), set(vec!["DE", "FR", "GB", "RU", "US"]), "каждая страна должна встречаться один раз:\n{t}");
    }

    #[tokio::test]
    async fn filtered_distinct() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(popular_authors()).await;
        assert_eq!(set(t.column(0)), set(vec!["1", "3", "6", "7", "11"]), "ждём по одному разу каждого автора:\n{t}");
    }

    #[tokio::test]
    async fn whole_row_distinct() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(country_status_pairs()).await;
        assert_eq!(t.len(), 9, "уникальных пар «страна и статус» девять:\n{t}");
    }
}
