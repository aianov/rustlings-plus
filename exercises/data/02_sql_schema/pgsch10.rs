// Комментарии к постам образуют ветки: у ответа есть parent_id. В поддержку пишут: «автор
// удалил свой комментарий, и пропали все ответы под ним, включая чужие, хотя люди писали их
// долго». Ответы должны остаться, просто потеряв родителя. А вот когда удаляют сам пост,
// его комментарии должны исчезнуть все, как и раньше.
// TODO: Найди, почему так происходит, и исправь схему.

fn comments_table() -> &'static str {
    "CREATE TABLE posts (
        id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
        body text NOT NULL
    );
    CREATE TABLE comments (
        id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
        post_id bigint NOT NULL REFERENCES posts (id) ON DELETE CASCADE,
        parent_id bigint REFERENCES comments (id) ON DELETE CASCADE,
        body text NOT NULL
    )"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;


    async fn cell(db: &TestDb, sql: &str) -> String {
        db.table(sql).await.cell(0, 0).to_string()
    }

    async fn prepared() -> TestDb {
        let db = TestDb::new().await;
        db.run(comments_table()).await;
        db.run(
            "INSERT INTO posts (body) VALUES ('p');
             INSERT INTO comments (post_id, parent_id, body) VALUES (1, NULL, 'root'), (1, 1, 'reply'), (1, 2, 'reply to reply')",
        )
        .await;
        db
    }

    #[tokio::test]
    async fn replies_survive_deleted_parent() {
        let db = prepared().await;
        db.run("DELETE FROM comments WHERE id = 1").await;
        let t = db.table("SELECT id, parent_id FROM comments ORDER BY id").await;
        assert_eq!(t.rows, [["2", "NULL"], ["3", "2"]], "ответы должны остаться: второй без родителя, третий под вторым:\n{t}");
    }

    #[tokio::test]
    async fn deleting_post_still_removes_all_comments() {
        let db = prepared().await;
        db.run("DELETE FROM posts WHERE id = 1").await;
        assert_eq!(cell(&db, "SELECT count(*) FROM comments").await, "0", "вместе с постом должны исчезнуть все его комментарии");
    }
}
