use scylla::Session;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

enum PostEvent {
    Created { post: i32, text: String, version_us: i64 },
    Deleted { post: i32, version_us: i64 },
}

async fn project(session: &Session, followers: &[i32], event: &PostEvent) -> Result<(), BoxError> {
    for user in followers {
        match event {
            PostEvent::Created { post, text, version_us } => {
                session
                    .query("INSERT INTO feed (user_id, post_id, text) VALUES (?, ?, ?) USING TIMESTAMP ?", (user, post, text, version_us))
                    .await?;
            }
            PostEvent::Deleted { post, version_us } => {
                session.query("DELETE FROM feed USING TIMESTAMP ? WHERE user_id = ? AND post_id = ?", (version_us, user, post)).await?;
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
