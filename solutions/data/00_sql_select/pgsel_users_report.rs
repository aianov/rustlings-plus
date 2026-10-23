fn recent_active() -> &'static str {
    "SELECT username, last_login_at
     FROM users
     WHERE status = 'active' AND last_login_at IS NOT NULL
     ORDER BY last_login_at DESC
     LIMIT 5"
}

fn needs_review() -> &'static str {
    "SELECT username AS login, status
     FROM users
     WHERE status IN ('pending', 'banned') OR last_login_at IS NULL
     ORDER BY status, username"
}

fn target_audience() -> &'static str {
    "SELECT email
     FROM users
     WHERE status = 'active'
       AND country IN ('DE', 'FR')
       AND age BETWEEN 20 AND 40
       AND email NOT LIKE '%@mail.org'"
}

fn top_posts() -> &'static str {
    "SELECT id, likes FROM posts ORDER BY likes DESC, id LIMIT 4"
}

fn users_page() -> &'static str {
    "SELECT username FROM users WHERE country = $1 ORDER BY created_at LIMIT $2 OFFSET $3"
}

fn active_countries() -> &'static str {
    "SELECT DISTINCT country AS code FROM users WHERE status = 'active' ORDER BY code"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn recent_active_logins() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(recent_active()).await;
        assert_eq!(t.columns, ["username", "last_login_at"], "нужны столбцы username и last_login_at");
        assert_eq!(
            t.col("username"),
            ["Karl", "grace", "frank", "bob_2", "alice"],
            "пять свежих заходов среди активных:\n{t}"
        );
    }

    #[tokio::test]
    async fn moderation_queue() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(needs_review()).await;
        assert_eq!(t.columns, ["login", "status"], "нужны столбцы login и status");
        assert_eq!(
            t.col("login"),
            ["dave", "carol", "judy", "eve", "heidi"],
            "pending, banned и не заходившие, порядок: статус, потом ник. Проверь скобки и NULL:\n{t}"
        );
    }

    #[tokio::test]
    async fn mailing_list() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(target_audience()).await;
        let mut got = t.column(0);
        got.sort();
        assert_eq!(got, ["alice@example.com", "grace@example.com"], "под условия подходят alice и grace:\n{t}");
    }

    #[tokio::test]
    async fn top_four_posts() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(top_posts()).await;
        assert_eq!(t.col("id"), ["2", "4", "11", "8"], "три поста по 30 лайков идут по возрастанию id, потом пост с 25:\n{t}");
        assert_eq!(t.columns, ["id", "likes"], "нужны столбцы id и likes");
    }

    #[tokio::test]
    async fn users_of_country_page() {
        let db = TestDb::with_sql(SEED).await;
        assert!(users_page().contains("$3"), "нужны три параметра: $1, $2 и $3");
        let got: Vec<String> = sqlx::query_scalar(users_page())
            .bind("US")
            .bind(2i64)
            .bind(1i64)
            .fetch_all(&db.pool)
            .await
            .unwrap_or_else(|e| panic!("запрос с тремя параметрами не выполнился: {e}"));
        assert_eq!(got, ["bob", "judy"], "из US по дате регистрации: bob_2, bob, judy, eve; страница из двух после пропуска одного");
    }

    #[tokio::test]
    async fn distinct_active_countries() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(active_countries()).await;
        assert_eq!(t.columns, ["code"], "столбец называется code");
        assert_eq!(t.column(0), ["DE", "FR", "GB", "RU", "US"], "страны по алфавиту без повторов:\n{t}");
    }
}
