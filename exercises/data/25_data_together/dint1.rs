// Публикация поста пишет пост, отправляет событие «пост создан» и увеличивает счётчик постов автора, всё
// в одной транзакции. Если автора нет, счётчик не обновляется, и транзакция откатывается. Но событие к этому
// моменту уже ушло, и подписчики строят ленты по посту, которого не существует. Почини порядок так, чтобы
// событие уходило только о посте, который действительно сохранён.

use sqlx::PgPool;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug, Clone, PartialEq)]
struct Event {
    id: i64,
    aggregate_id: i64,
    kind: String,
}

trait Publisher: Send + Sync {
    fn publish(&self, event: &Event) -> Result<(), String>;
}

async fn create_post(db: &PgPool, bus: &dyn Publisher, id: i64, author: i64, text: &str) -> Result<(), BoxError> {
    let mut tx = db.begin().await?;
    sqlx::query("INSERT INTO posts (id, author, text) VALUES ($1, $2, $3)").bind(id).bind(author).bind(text).execute(&mut *tx).await?;
    bus.publish(&Event { id, aggregate_id: id, kind: "post_created".to_string() })?;
    let n = sqlx::query("UPDATE authors SET posts = posts + 1 WHERE id = $1").bind(author).execute(&mut *tx).await?.rows_affected();
    if n == 0 {
        tx.rollback().await?;
        return Err("автора нет".into());
    }
    tx.commit().await?;
    Ok(())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    use std::sync::Mutex;

    #[derive(Default)]
    struct Bus {
        sent: Mutex<Vec<Event>>,
        fail_on: Mutex<Option<usize>>,
        delay_ms: u64,
        slow_created: bool,
    }

    impl Publisher for Bus {
        fn publish(&self, event: &Event) -> Result<(), String> {
            if event.kind.is_empty() {
                return Err("событие без типа".to_string());
            }
            let delay = if self.slow_created && event.kind == "post_created" { 200 } else { self.delay_ms };
            if delay > 0 {
                std::thread::sleep(std::time::Duration::from_millis(delay));
            }
            let mut sent = self.sent.lock().unwrap();
            if *self.fail_on.lock().unwrap() == Some(sent.len()) {
                *self.fail_on.lock().unwrap() = None;
                return Err("брокер недоступен".to_string());
            }
            sent.push(event.clone());
            Ok(())
        }
    }

    const SEED: &str = "CREATE TABLE posts (id bigint PRIMARY KEY, author bigint NOT NULL, text text NOT NULL); CREATE TABLE authors (id bigint PRIMARY KEY, posts int NOT NULL DEFAULT 0); INSERT INTO authors (id) VALUES (7)";

    #[tokio::test]
    async fn no_event_about_a_rolled_back_post() {
        let db = TestDb::with_sql(SEED).await;
        let bus = Bus::default();
        assert!(create_post(&db.pool, &bus, 1, 99, "от несуществующего автора").await.is_err(), "автора 99 нет, пост не создаётся");
        assert!(db.table("SELECT id FROM posts").await.is_empty(), "пост откатился");
        assert!(bus.sent.lock().unwrap().is_empty(), "о несохранённом посте никто не должен узнать");
        create_post(&db.pool, &bus, 2, 7, "настоящий пост").await.expect("обычный пост");
        assert_eq!(bus.sent.lock().unwrap().len(), 1, "о сохранённом посте событие ушло");
    }
}
