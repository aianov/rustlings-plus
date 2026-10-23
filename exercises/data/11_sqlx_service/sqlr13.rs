// Массовые лайки пачкой: список троек (пост, пользователь, вес) вставляется через QueryBuilder.
// Для сотни строк всё работает, но когда пришёл ночной импорт на тридцать тысяч, база ответила
// ошибкой про число параметров. Кроме того, пустой список тоже не должен приводить к ошибке,
// и вся партия должна быть всё-или-ничего: если одна строка плохая, не должно остаться ни одной.
// TODO: Найди предел и исправь функцию.

use sqlx::{PgPool, QueryBuilder};

async fn add_likes(pool: &PgPool, rows: &[(i64, i64, i32)]) -> Result<u64, sqlx::Error> {
    let mut qb = QueryBuilder::new("INSERT INTO likes (post_id, user_id, weight) ");
    qb.push_values(rows, |mut b, r| {
        b.push_bind(r.0).push_bind(r.1).push_bind(r.2);
    });
    Ok(qb.build().execute(pool).await?.rows_affected())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    const MORE: &str = "
        INSERT INTO users (username) SELECT 'bulk' || g FROM generate_series(1, 200) g;
        INSERT INTO posts (author_id, body) SELECT 1, 'p' || g FROM generate_series(1, 150) g;
    ";

    fn pairs() -> Vec<(i64, i64, i32)> {
        let mut rows = Vec::new();
        for post in 9..159 {
            for user in 5..205 {
                rows.push((post, user, 1));
            }
        }
        rows
    }

    #[tokio::test]
    async fn thirty_thousand_rows_fit() {
        let db = TestDb::with_sql(SEED).await;
        db.run(MORE).await;
        let rows = pairs();
        assert_eq!(rows.len(), 30_000, "для проверки нужно 30 тысяч строк");
        let added = add_likes(&db.pool, &rows).await.expect("30 тысяч строк должны пройти: у Postgres не больше 65535 параметров в одной команде, а здесь их 90000");
        assert_eq!(added, 30_000, "должны быть добавлены все строки");
        let t = db.table("SELECT count(*) FROM likes WHERE post_id >= 9").await;
        assert_eq!(t.cell(0, 0), "30000", "в таблице должно быть 30000 новых лайков");
    }

    #[tokio::test]
    async fn empty_list_is_ok() {
        let db = TestDb::with_sql(SEED).await;
        let added = add_likes(&db.pool, &[]).await.expect("пустой список не должен быть ошибкой");
        assert_eq!(added, 0, "пустой список ничего не добавляет");
    }

    #[tokio::test]
    async fn one_bad_row_cancels_every_chunk() {
        let db = TestDb::with_sql(SEED).await;
        db.run(MORE).await;
        let mut rows = pairs();
        rows.push((9, 99_999, 1));
        let res = add_likes(&db.pool, &rows).await;
        assert!(res.is_err(), "последняя строка ссылается на несуществующего пользователя, партия должна упасть");
        let t = db.table("SELECT count(*) FROM likes WHERE post_id >= 9").await;
        assert_eq!(t.cell(0, 0), "0", "ошибка в последней пачке не должна оставить в базе первые: вся партия обязана быть атомарной");
    }
}
