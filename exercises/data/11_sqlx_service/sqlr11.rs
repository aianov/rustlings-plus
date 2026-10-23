// Пользователь просит выгрузить все свои посты. Их может быть сотни тысяч, и сервис не должен
// сначала загрузить всё в память, а потом отдавать. Нужно отдавать посты по мере прихода из базы:
// `on_post` вызывается на каждый пост и возвращает false, если получателю больше не нужно
// (клиент отключился). Функция возвращает, сколько постов передала в `on_post`.
// Сейчас она читает всё сразу. В своём проекте поток читают через `futures::TryStreamExt`,
// а здесь он доступен как `course_support::pg::TryStreamExt`.
// TODO: Перепиши чтение потоком. Запрос в EXPORT не меняй, метка в комментарии нужна тесту.

use sqlx::PgPool;

const EXPORT: &str = "SELECT /* sqlr11 */ id, body FROM posts WHERE author_id = $1 ORDER BY id";

async fn export_posts(pool: &PgPool, author_id: i64, mut on_post: impl FnMut(i64, String) -> bool) -> Result<u64, sqlx::Error> {
    let rows: Vec<(i64, String)> = sqlx::query_as(EXPORT).bind(author_id).fetch_all(pool).await?;
    let mut sent = 0;
    for (id, body) in rows {
        sent += 1;
        if !on_post(id, body) {
            break;
        }
    }
    Ok(sent)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn posts_come_in_order_and_early_stop_works() {
        let db = TestDb::with_sql(SEED).await;
        let one = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_secs(3))
            .connect_with(db.options())
            .await
            .unwrap();
        let mut got = Vec::new();
        let sent = export_posts(&one, 1, |id, _| {
            got.push(id);
            true
        })
        .await
        .expect("выгрузка постов anna");
        assert_eq!((sent, got.clone()), (3, vec![1, 2, 8]), "у anna три поста, их id по возрастанию: 1, 2, 8");
        let mut first = Vec::new();
        let sent = export_posts(&one, 1, |id, _| {
            first.push(id);
            false
        })
        .await
        .expect("выгрузка с остановкой");
        assert_eq!((sent, first), (1, vec![1]), "после false читать дальше не нужно, переданным считается один пост");
        let again = export_posts(&one, 1, |_, _| true).await.expect("после остановки соединение должно вернуться в пул");
        assert_eq!(again, 3, "повторная выгрузка на том же единственном соединении должна работать");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 3)]
    async fn rows_are_not_all_loaded_before_the_first_callback() {
        let db = TestDb::with_sql(SEED).await;
        db.run("INSERT INTO posts (author_id, body, status) SELECT 4, repeat('x', 200) || g, 'published' FROM generate_series(1, 150000) g").await;
        let first = Arc::new(AtomicBool::new(false));
        let done = Arc::new(AtomicBool::new(false));
        let seen = Arc::new(AtomicBool::new(false));
        let poller = {
            let (pool, first, done, seen) = (db.pool.clone(), first.clone(), done.clone(), seen.clone());
            tokio::spawn(async move {
                while !done.load(Ordering::SeqCst) {
                    if first.load(Ordering::SeqCst) {
                        let n: i64 = sqlx::query_scalar(
                            "SELECT count(*) FROM pg_stat_activity WHERE state = 'active' AND query LIKE '%sqlr11%' AND query NOT LIKE '%pg_stat_activity%'",
                        )
                        .fetch_one(&pool)
                        .await
                        .unwrap_or(0);
                        if n > 0 {
                            seen.store(true, Ordering::SeqCst);
                        }
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            })
        };
        let flag = first.clone();
        let sent = export_posts(&db.pool, 4, move |_, _| {
            if !flag.swap(true, Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(600));
            }
            true
        })
        .await
        .expect("выгрузка большого числа постов");
        done.store(true, Ordering::SeqCst);
        poller.await.unwrap();
        assert_eq!(sent, 150_002, "должны быть переданы все посты dmitry: два из seed и 150000 добавленных");
        assert!(
            seen.load(Ordering::SeqCst),
            "когда пришёл первый пост, запрос на сервере уже закончился: функция сначала загрузила все строки в память, а потом отдавала. Читай поток через fetch"
        );
    }
}
