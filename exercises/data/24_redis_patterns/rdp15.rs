// Лайки копятся в Redis (HINCRBY в хеш «ожидающих» по id поста), а фоновая задача раз в секунду переносит их
// в Postgres и обнуляет. Сейчас она читает хеш, пишет в базу и удаляет хеш, и лайки, пришедшие между чтением
// и удалением, пропадают. Почини `flush`: сначала атомарно переименуй хеш ожидающих в рабочий ключ (новые
// лайки начнут копиться в новом хеше), потом спокойно перенеси рабочий ключ в базу и удали его.

use course_support::redis::Client;
use sqlx::PgPool;
use std::time::Duration;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

async fn like(c: &Client, ns: &str, post: i64) -> Result<(), BoxError> {
    c.cmd(&["HINCRBY", &format!("{ns}:likes:pending"), &post.to_string(), "1"]).await?;
    Ok(())
}

async fn flush(db: &PgPool, c: &Client, ns: &str) -> Result<i64, BoxError> {
    let key = format!("{ns}:likes:pending");
    let pairs = c.cmd(&["HGETALL", &key]).await?.texts();
    let mut moved = 0;
    for p in pairs.chunks(2) {
        let (Some(post), Some(n)) = (p.first().and_then(|v| v.parse::<i64>().ok()), p.get(1).and_then(|v| v.parse::<i64>().ok())) else {
            continue;
        };
        sqlx::query("UPDATE posts SET likes = likes + $1 WHERE id = $2").bind(n).bind(post).execute(db).await?;
        moved += n;
    }
    tokio::time::sleep(Duration::from_millis(100)).await;
    c.cmd(&["DEL", &key]).await?;
    Ok(moved)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;
    use course_support::pg::TestDb;

    const SEED: &str = "CREATE TABLE posts (id bigint PRIMARY KEY, likes bigint NOT NULL DEFAULT 0); INSERT INTO posts (id) VALUES (1), (2)";

    #[tokio::test]
    async fn no_like_is_lost() {
        let db = TestDb::with_sql(SEED).await;
        let s = Scope::new().await;
        let (c, ns) = (s.client(), s.prefix().to_string());
        let liker = {
            let (c, ns) = (s.client(), ns.clone());
            tokio::spawn(async move {
                for i in 0..300 {
                    like(&c, &ns, 1 + i % 2).await.map_err(|e| e.to_string())?;
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
                Ok::<(), String>(())
            })
        };
        for _ in 0..5 {
            tokio::time::sleep(Duration::from_millis(60)).await;
            flush(&db.pool, &c, &ns).await.expect("перенос");
        }
        liker.await.expect("задача").expect("лайки");
        flush(&db.pool, &c, &ns).await.expect("последний перенос");
        let total = db.table("SELECT sum(likes)::text FROM posts").await;
        assert_eq!(total.cell(0, 0), "300", "все 300 лайков дошли до базы\n{total}");
        assert_eq!(flush(&db.pool, &c, &ns).await.expect("пустой перенос"), 0, "перенос без ожидающих лайков ничего не делает");
    }
}
