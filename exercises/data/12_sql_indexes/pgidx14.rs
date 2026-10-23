// В ленте листаются посты по пять штук, курсор это время последнего поста страницы. У пользователей,
// которые публикуют пачками, у нескольких постов бывает одно и то же время. Читатели жалуются:
// «некоторые посты вообще не показываются».
// Запрос принимает $1 (время курсора) и, когда нужно, $2 (id последнего поста) и возвращает
// `id` и `created_at`. Порядок «время и id по убыванию».
// TODO: Исправь запрос так, чтобы страницы по курсору давали все посты по одному разу.

fn page_sql() -> &'static str {
    "SELECT id, created_at FROM posts WHERE created_at < $1 ORDER BY created_at DESC, id DESC LIMIT 5"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    use chrono::{DateTime, Duration, Utc};

    const TABLE: &str = "
        CREATE TABLE posts (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, created_at timestamptz NOT NULL);
        INSERT INTO posts (created_at) SELECT timestamptz '2026-01-01 12:00:00+00' + ((g / 4) || ' minutes')::interval FROM generate_series(1, 22) g;
        CREATE INDEX posts_feed_idx ON posts (created_at DESC, id DESC);
    ";

    #[tokio::test]
    async fn every_post_exactly_once() {
        let db = TestDb::with_sql(TABLE).await;
        let sql = page_sql();
        let mut cursor: (DateTime<Utc>, i64) = (Utc::now() + Duration::days(3650), i64::MAX);
        let mut seen: Vec<i64> = Vec::new();
        for _ in 0..10 {
            let mut q = sqlx::query_as::<_, (i64, DateTime<Utc>)>(sql).bind(cursor.0);
            if sql.contains("$2") {
                q = q.bind(cursor.1);
            }
            let rows = q.fetch_all(&db.pool).await.expect("страница ленты");
            if rows.is_empty() {
                break;
            }
            seen.extend(rows.iter().map(|r| r.0));
            let last = rows.last().unwrap();
            cursor = (last.1, last.0);
        }
        let expected = db.table("SELECT id FROM posts ORDER BY created_at DESC, id DESC").await;
        let expected: Vec<i64> = expected.column(0).iter().map(|s| s.parse().unwrap()).collect();
        assert_eq!(seen, expected, "постов 22, у четырёх подряд одинаковое время; по курсору должны выйти все 22, по одному разу, в порядке ленты. Сейчас выходит: {} постов", seen.len());
    }

    #[tokio::test]
    async fn the_order_stays_stable() {
        let db = TestDb::with_sql(TABLE).await;
        let sql = page_sql();
        let mut q = sqlx::query_as::<_, (i64, DateTime<Utc>)>(sql).bind(Utc::now() + Duration::days(3650));
        if sql.contains("$2") {
            q = q.bind(i64::MAX);
        }
        let rows = q.fetch_all(&db.pool).await.expect("первая страница");
        let ids: Vec<i64> = rows.iter().map(|r| r.0).collect();
        assert_eq!(ids, vec![22, 21, 20, 19, 18], "первая страница это пять самых новых постов, при равном времени больший id раньше");
    }
}
