fn profile_view() -> &'static str {
    "SELECT user_id,
            settings ->> 'theme' AS theme,
            settings #>> '{ui,lang}' AS lang,
            settings #>> '{notifications,email}' AS email
     FROM profiles
     ORDER BY user_id"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = r#"
        CREATE TABLE profiles (user_id bigint PRIMARY KEY, settings jsonb NOT NULL);
        INSERT INTO profiles VALUES
            (1, '{"theme":"dark","ui":{"lang":"ru"},"notifications":{"email":true,"push":false}}'),
            (2, '{"theme":"light","ui":{"lang":"en"},"notifications":{"email":false}}'),
            (3, '{"theme":"dark"}'),
            (4, '{}');
    "#;

    #[tokio::test]
    async fn reads_flat_and_nested_keys() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(profile_view()).await;
        assert_eq!(t.columns, vec!["user_id", "theme", "lang", "email"], "нужны ровно эти столбцы в этом порядке:\n{t}");
        let expected: Vec<Vec<&str>> = vec![
            vec!["1", "dark", "ru", "true"],
            vec!["2", "light", "en", "false"],
            vec!["3", "dark", "NULL", "NULL"],
            vec!["4", "NULL", "NULL", "NULL"],
        ];
        assert_eq!(t.rows, expected, "значения должны быть обычным текстом (->> и #>>, а не -> и #>), пропавшие ключи дают NULL:\n{t}");
    }

    #[tokio::test]
    async fn works_for_rows_added_later() {
        let db = TestDb::with_sql(SEED).await;
        db.run(r#"INSERT INTO profiles VALUES (5, '{"theme":"auto","ui":{"lang":"de"},"notifications":{"email":true}}')"#).await;
        let t = db.table(profile_view()).await;
        assert_eq!(t.rows.last().cloned(), Some(vec!["5".to_string(), "auto".into(), "de".into(), "true".into()]), "новая строка тоже должна читаться запросом:\n{t}");
    }
}
