// Одобрение черновика: пост становится опубликованным, в audit пишется строка, потом автору
// уходит внешнее уведомление (SMS-шлюз бывает медленным, а бывает недоступным). Пока уведомление
// идёт, никто не может поставить лайк этому посту, строка заперта. Одобрение при этом уже
// состоялось и откатываться из-за шлюза не должно.
// TODO: Найди, что держит блокировку, и исправь порядок действий.

use sqlx::PgPool;
use std::future::Future;

async fn approve_post<F, Fut>(pool: &PgPool, post_id: i64, notify: F) -> Result<bool, sqlx::Error>
where
    F: FnOnce(i64) -> Fut,
    Fut: Future<Output = Result<(), String>>, {
    let mut tx = pool.begin().await?;
    let changed = sqlx::query("UPDATE posts SET status = 'published' WHERE id = $1 AND status = 'draft'")
        .bind(post_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if changed == 0 {
        return Ok(false);
    }
    sqlx::query("INSERT INTO audit (note) VALUES ($1)")
        .bind(format!("approved {post_id}"))
        .execute(&mut *tx)
        .await?;
    let _ = notify(post_id).await;
    tx.commit().await?;
    Ok(true)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;
    use tokio::sync::oneshot;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn row_lock_is_free_while_the_gateway_works() {
        let db = TestDb::with_sql(SEED).await;
        let (entered_tx, entered_rx) = oneshot::channel::<()>();
        let (release_tx, release_rx) = oneshot::channel::<()>();
        let pool = db.pool.clone();
        let job = tokio::spawn(async move {
            approve_post(&pool, 7, move |_| async move {
                let _ = entered_tx.send(());
                let _ = release_rx.await;
                Err("шлюз не отвечает".to_string())
            })
            .await
        });
        tokio::time::timeout(Duration::from_secs(5), entered_rx)
            .await
            .expect("уведомление не вызвали за 5 секунд")
            .expect("approve_post закончилась, так и не вызвав уведомление");
        let mut conn = db.pool.acquire().await.unwrap();
        sqlx::query("SET lock_timeout = '500ms'").execute(&mut *conn).await.unwrap();
        let like = sqlx::query("UPDATE posts SET likes_count = likes_count + 1 WHERE id = 7").execute(&mut *conn).await;
        release_tx.send(()).unwrap();
        let approved = job.await.unwrap().expect("одобрение должно завершиться без ошибки базы");
        assert!(
            like.is_ok(),
            "пока шло внешнее уведомление, строка поста оставалась заблокирована: транзакция ещё не зафиксирована, а вызов во внешний мир уже идёт"
        );
        assert!(approved, "черновик 7 должен быть одобрен, несмотря на недоступный шлюз");
    }

    #[tokio::test]
    async fn gateway_failure_does_not_undo_the_approval() {
        let db = TestDb::with_sql(SEED).await;
        let ok = approve_post(&db.pool, 4, |_| async { Err("шлюз упал".to_string()) }).await.expect("одобрение");
        assert!(ok, "черновик 4 должен быть одобрен");
        let t = db.table("SELECT status FROM posts WHERE id = 4").await;
        assert_eq!(t.cell(0, 0), "published", "статус должен измениться, сбой уведомления его не отменяет");
        let t = db.table("SELECT note FROM audit").await;
        assert_eq!(t.col("note"), vec!["approved 4"], "запись в audit должна остаться");
    }

    #[tokio::test]
    async fn published_post_is_not_approved_twice() {
        let db = TestDb::with_sql(SEED).await;
        let calls = Arc::new(AtomicU32::new(0));
        let seen = calls.clone();
        let ok = approve_post(&db.pool, 1, move |_| async move {
            seen.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
        .await
        .expect("повторное одобрение");
        assert!(!ok, "пост 1 уже опубликован, одобрять нечего");
        assert_eq!(calls.load(Ordering::SeqCst), 0, "уведомление про уже опубликованный пост отправлять не нужно");
        let t = db.table("SELECT count(*) FROM audit").await;
        assert_eq!(t.cell(0, 0), "0", "записи в audit быть не должно");
    }
}
