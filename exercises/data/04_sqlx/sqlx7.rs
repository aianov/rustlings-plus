// Лента постов показывает карточки: автор, текст, время. Структура PostCard уже используется
// в шаблонах по всему приложению, её поля переименовывать нельзя. Запрос собран и выглядит
// разумно, но при первом же вызове падает ошибкой про отсутствующий столбец.
// TODO: Почини, чтобы карточки читались, не трогая поля PostCard.

use chrono::{DateTime, Utc};
use sqlx::PgPool;

#[derive(Debug, sqlx::FromRow)]
struct PostCard {
    id: i64,
    author_name: String,
    text: String,
    posted_at: DateTime<Utc>,
}

async fn latest_cards(pool: &PgPool, limit: i64) -> Result<Vec<PostCard>, sqlx::Error> {
    sqlx::query_as::<_, PostCard>(
        "SELECT p.id, u.username, p.body, p.created_at
         FROM posts p JOIN users u ON u.id = p.author_id
         ORDER BY p.created_at DESC, p.id DESC
         LIMIT $1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn cards_are_read() {
        let db = TestDb::with_sql(SEED).await;
        let cards = latest_cards(&db.pool, 3).await.expect("карточки должны читаться: имена столбцов результата должны совпасть с полями структуры");
        let got: Vec<(i64, &str, &str)> = cards.iter().map(|c| (c.id, c.author_name.as_str(), c.text.as_str())).collect();
        assert_eq!(got, [(8, "eve", "Новая тема"), (5, "alice", "Итоги месяца"), (2, "alice", "sqlx это просто")], "три самых новых поста");
    }

    #[tokio::test]
    async fn time_is_read_as_utc() {
        let db = TestDb::with_sql(SEED).await;
        let cards = latest_cards(&db.pool, 1).await.expect("запрос должен выполниться");
        assert_eq!(cards[0].posted_at.to_rfc3339(), "2024-06-02T10:00:00+00:00", "момент времени должен прочитаться в UTC без сдвига");
    }
}
