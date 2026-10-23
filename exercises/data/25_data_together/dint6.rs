// Потребитель события «лайк» увеличивает счётчик лайков поста. Доставка «хотя бы один раз» иногда приносит
// одно событие дважды, и счётчик завышается. Inbox: потребитель записывает id обработанного события в таблицу
// inbox той же транзакцией, что и эффект, и пропускает событие, если его id уже там. Дубли, пришедшие
// одновременно, тоже должны примениться один раз.
// TODO: `handle` возвращает true, если эффект применён, и false, если событие уже было.

use sqlx::PgPool;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

async fn handle(db: &PgPool, event_id: i64, post: i64) -> Result<bool, BoxError> {
    let _ = (db, event_id, post);
    todo!("inbox и эффект одной транзакцией")
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = "CREATE TABLE posts (id bigint PRIMARY KEY, likes bigint NOT NULL DEFAULT 0); CREATE TABLE inbox (event_id bigint PRIMARY KEY, handled_at timestamptz NOT NULL DEFAULT now()); INSERT INTO posts (id) VALUES (1)";

    #[tokio::test]
    async fn duplicates_are_skipped() {
        let db = TestDb::with_sql(SEED).await;
        assert!(handle(&db.pool, 100, 1).await.expect("первый раз"), "новое событие применяется");
        assert!(!handle(&db.pool, 100, 1).await.expect("дубль"), "дубль пропускается");
        assert!(handle(&db.pool, 101, 1).await.expect("другое событие"));
        assert_eq!(db.table("SELECT likes FROM posts").await.cell(0, 0), "2");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_duplicates_apply_once() {
        let db = TestDb::with_sql(SEED).await;
        let mut tasks = Vec::new();
        for _ in 0..10 {
            let pool = db.pool.clone();
            tasks.push(tokio::spawn(async move { handle(&pool, 500, 1).await.map_err(|e| e.to_string()) }));
        }
        let mut applied = 0;
        for t in tasks {
            if t.await.expect("задача").expect("обработка") {
                applied += 1;
            }
        }
        assert_eq!(applied, 1, "из десяти одновременных дублей применён один");
        assert_eq!(db.table("SELECT likes FROM posts").await.cell(0, 0), "1");
    }
}
