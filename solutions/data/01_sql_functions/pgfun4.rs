fn preview() -> &'static str {
    "SELECT id, LEFT(body, 8) AS preview FROM posts"
}

fn mail_tld() -> &'static str {
    "SELECT username, SPLIT_PART(SPLIT_PART(email, '@', 2), '.', 2) AS tld FROM users"
}

fn slug() -> &'static str {
    "SELECT id, REPLACE(LOWER(body), ' ', '-') AS slug FROM posts"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::{Table, TestDb};

    const SEED: &str = include_str!("seed.sql");

    fn row<'a>(t: &'a Table, key: &str) -> &'a Vec<String> {
        t.rows
            .iter()
            .find(|r| r[0] == key)
            .unwrap_or_else(|| panic!("в результате нет строки с ключом {key}:\n{t}"))
    }

    #[tokio::test]
    async fn left_part() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(preview()).await;
        assert_eq!(row(&t, "1")[1], "Hello wo", "первые 8 символов поста 1:\n{t}");
    }

    #[tokio::test]
    async fn nested_split() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(mail_tld()).await;
        assert_eq!(row(&t, "bob")[1], "org", "у bob@mail.org зона org:\n{t}");
        assert_eq!(row(&t, "kenji")[1], "jp", "у kenji@example.jp зона jp:\n{t}");
        assert_eq!(row(&t, "alice")[1], "com");
    }

    #[tokio::test]
    async fn replace_spaces() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(slug()).await;
        assert_eq!(row(&t, "2")[1], "rust-is-fun", "слаг поста 2:\n{t}");
        assert_eq!(row(&t, "3")[1], "coffee-first,-code-later", "запятая остаётся, меняются только пробелы:\n{t}");
    }
}
