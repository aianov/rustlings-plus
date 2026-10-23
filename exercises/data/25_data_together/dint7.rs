// Лента в Scylla строится проекцией событий: «пост создан» кладёт пост в ленты подписчиков, «пост удалён»
// убирает. События могут прийти повторно и не по порядку (повтор после сбоя relay обгоняет следующее событие).
// Удалённый пост не должен воскреснуть от запоздавшего «создан». У каждого события есть версия: момент события
// в микросекундах. Запиши её как метку времени записи, и Scylla сама оставит более позднюю версию.
// TODO: `project` применяет событие к лентам `followers`.

use scylla::Session;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

enum PostEvent {
    Created { post: i32, text: String, version_us: i64 },
    Deleted { post: i32, version_us: i64 },
}

async fn project(session: &Session, followers: &[i32], event: &PostEvent) -> Result<(), BoxError> {
    for user in followers {
        match event {
            PostEvent::Created { post, text, .. } => {
                session.query("INSERT INTO feed (user_id, post_id, text) VALUES (?, ?, ?)", (user, post, text)).await?;
            }
            PostEvent::Deleted { post, .. } => {
                session.query("DELETE FROM feed WHERE user_id = ? AND post_id = ?", (user, post)).await?;
            }
        }
    }
    Ok(())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    const SCHEMA: &str = "CREATE TABLE feed (user_id int, post_id int, text text, PRIMARY KEY (user_id, post_id))";

    fn now_us() -> i64 {
        let us = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_micros()).unwrap_or(0);
        i64::try_from(us).unwrap_or(i64::MAX) - 60_000_000
    }

    #[tokio::test]
    async fn late_create_does_not_resurrect() {
        let ks = TestKs::with_cql(SCHEMA).await;
        let base = now_us();
        let created = PostEvent::Created { post: 1, text: "привет".to_string(), version_us: base };
        let deleted = PostEvent::Deleted { post: 1, version_us: base + 1_000 };
        project(&ks.session, &[7, 9], &deleted).await.expect("удаление пришло первым");
        project(&ks.session, &[7, 9], &created).await.expect("создание опоздало");
        let t = ks.table("SELECT post_id FROM feed WHERE user_id = 7").await;
        assert!(t.is_empty(), "удалённый пост не воскрес от запоздавшего события\n{t}");
    }

    #[tokio::test]
    async fn repeats_are_harmless() {
        let ks = TestKs::with_cql(SCHEMA).await;
        let base = now_us();
        let created = PostEvent::Created { post: 2, text: "второй".to_string(), version_us: base };
        for _ in 0..3 {
            project(&ks.session, &[7], &created).await.expect("повтор");
        }
        let t = ks.table("SELECT post_id, text FROM feed WHERE user_id = 7").await;
        assert_eq!(t.rows, [["2", "второй"]], "повторы дают одну строку\n{t}");
    }
}
