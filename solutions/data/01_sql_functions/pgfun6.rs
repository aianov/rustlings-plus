fn heat() -> &'static str {
    "SELECT id,
            CASE
                WHEN likes >= 100 THEN 'hot'
                WHEN likes >= 20 THEN 'popular'
                WHEN likes > 0 THEN 'normal'
                ELSE 'quiet'
            END AS heat
     FROM posts"
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
    async fn labels() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(heat()).await;
        for (id, want) in [("7", "hot"), ("10", "hot"), ("9", "popular"), ("2", "popular"), ("19", "popular"), ("6", "normal"), ("12", "normal"), ("3", "normal"), ("5", "quiet")] {
            assert_eq!(row(&t, id)[1], want, "метка поста {id}: границы включаются слева (100 уже hot, 99 ещё popular, 20 уже popular, 1 уже normal):\n{t}");
        }
    }
}
