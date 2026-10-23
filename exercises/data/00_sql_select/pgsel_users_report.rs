// Отчёт для админки соцсети. Шесть запросов по условию, всё из этой главы: фильтры, скобки,
// NULL, сортировка, страницы, параметры. В файле только заглушки: пиши запросы с нуля.
// Каждый запрос описан над своей функцией, а тест сверяет точный ответ.

fn recent_active() -> &'static str {
    // 1. Пять активных пользователей (status = 'active'), заходивших последними. Столбцы:
    // username, last_login_at. Свежие первыми, не заходившие вообще в ответ не попадают.
    todo!()
}

fn needs_review() -> &'static str {
    // 2. Кого проверить модератору: статус 'pending' или 'banned', либо ни разу не заходил.
    // Столбцы: username под именем login и status. Сначала по статусу, внутри статуса по нику.
    todo!()
}

fn target_audience() -> &'static str {
    // 3. Рассылка: активные, из DE или FR, возраст от 20 до 40 включительно, а почта не на
    // @mail.org. Один столбец: email.
    todo!()
}

fn top_posts() -> &'static str {
    // 4. Четыре самых залайканных поста; при равных лайках сначала меньший id.
    // Столбцы: id, likes.
    todo!()
}

fn users_page() -> &'static str {
    // 5. Страница пользователей одной страны по дате регистрации, старые первыми.
    // $1 страна, $2 размер страницы, $3 сколько строк пропустить. Один столбец: username.
    todo!()
}

fn active_countries() -> &'static str {
    // 6. В каких странах есть активные пользователи, каждая страна один раз,
    // по алфавиту. Столбец назови code.
    todo!()
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
