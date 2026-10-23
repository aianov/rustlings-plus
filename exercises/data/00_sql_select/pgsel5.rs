// WHERE оставляет только те строки, для которых условие истинно. Условия собираются из
// сравнений (=, <>, <, <=, >, >=) и связок AND, OR, NOT.
// Все четыре запроса должны вернуть один столбец: username.

fn older_than_30() -> &'static str {
    // TODO: пользователи старше 30 лет
    "SELECT username FROM users"
}

fn active_germans() -> &'static str {
    // TODO: из Германии (DE) и со статусом 'active'
    "SELECT username FROM users"
}

fn french_or_british() -> &'static str {
    // TODO: из Франции (FR) или Великобритании (GB)
    "SELECT username FROM users"
}

fn not_active() -> &'static str {
    // TODO: все, у кого статус не 'active' (через NOT)
    "SELECT username FROM users"
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

    async fn names(sql: &str) -> Vec<String> {
        let db = TestDb::with_sql(SEED).await;
        db.table(sql).await.col("username").into_iter().map(String::from).collect()
    }

    fn check(got: Vec<String>, want: Vec<&str>, what: &str) {
        let got = set(got.iter().map(String::as_str).collect());
        assert_eq!(got, set(want), "{what}: ответ не совпал с ожидаемым");
    }

    #[tokio::test]
    async fn comparison() {
        check(names(older_than_30()).await, vec!["bob", "carol", "frank", "grace", "ivan", "Karl"], "age > 30");
    }

    #[tokio::test]
    async fn and_condition() {
        check(names(active_germans()).await, vec!["alice", "Karl"], "DE и active");
    }

    #[tokio::test]
    async fn or_condition() {
        check(names(french_or_british()).await, vec!["dave", "frank", "grace", "heidi"], "FR или GB");
    }

    #[tokio::test]
    async fn not_condition() {
        check(names(not_active()).await, vec!["carol", "eve", "heidi", "judy"], "NOT active");
    }
}
