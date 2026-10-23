fn percent() -> &'static str {
    "SELECT id, ROUND(likes * 100.0 / views, 1) AS pct FROM posts WHERE views > 0"
}

fn pages() -> &'static str {
    "SELECT id, CEIL(likes / 25.0) AS pages FROM posts"
}

fn exact_sum() -> &'static str {
    "SELECT 0.1::numeric + 0.2::numeric = 0.3::numeric AS same"
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
    async fn rounding() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(percent()).await;
        assert_eq!(row(&t, "7")[1], "13.9", "250 лайков на 1800 просмотров это 13.9%:\n{t}");
        assert_eq!(row(&t, "2")[1], "25.0", "30 на 120 это ровно 25.0%:\n{t}");
    }

    #[tokio::test]
    async fn ceiling() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(pages()).await;
        assert_eq!(row(&t, "7")[1], "10", "250 лайков это ровно 10 страниц:\n{t}");
        assert_eq!(row(&t, "2")[1], "2", "30 лайков: полная страница и ещё неполная:\n{t}");
        assert_eq!(row(&t, "8")[1], "1", "ровно 25 лайков помещаются на одну страницу:\n{t}");
        assert_eq!(row(&t, "19")[1], "1", "20 лайков это одна неполная страница:\n{t}");
        assert_eq!(row(&t, "5")[1], "0", "у поста без лайков страниц нет:\n{t}");
    }

    #[tokio::test]
    async fn numeric_is_exact() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(exact_sum()).await;
        assert_eq!(t.cell(0, 0), "true", "в numeric 0.1 + 0.2 равно 0.3 ровно:\n{t}");
    }
}
