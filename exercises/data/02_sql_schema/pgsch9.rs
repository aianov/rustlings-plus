// Внешние ключи уже стоят, но удаление работает по умолчанию: родителя с детьми не удалить.
// Нужны три разных поведения. Когда удаляют пост, его лайки не нужны и должны исчезнуть вместе
// с ним. Когда удаляют пользователя-модератора, посты остаются, просто никто их больше не
// модерирует. Пользователя, у которого есть покупки, удалять нельзя вообще: деньги должны
// оставаться привязанными к человеку.
// TODO: Для каждой ссылки выбери правило удаления.

fn delete_rules() -> &'static str {
    "CREATE TABLE users (
        id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
        username text NOT NULL
    );
    CREATE TABLE posts (
        id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
        author_id bigint NOT NULL,
        moderator_id bigint REFERENCES users (id)
    );
    CREATE TABLE likes (
        post_id bigint NOT NULL REFERENCES posts (id),
        user_id bigint NOT NULL
    );
    CREATE TABLE purchases (
        id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
        user_id bigint NOT NULL REFERENCES users (id)
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
        db.run(delete_rules()).await;
        db.run(
            "INSERT INTO users (username) VALUES ('author'), ('moderator'), ('buyer');
             INSERT INTO posts (author_id, moderator_id) VALUES (1, 2);
             INSERT INTO likes VALUES (1, 3), (1, 1);
             INSERT INTO purchases (user_id) VALUES (3)",
        )
        .await;
        db
    }

    #[tokio::test]
    async fn deleting_post_removes_its_likes() {
        let db = prepared().await;
        db.run("DELETE FROM posts WHERE id = 1").await;
        assert_eq!(cell(&db, "SELECT count(*) FROM likes").await, "0", "лайки удалённого поста должны исчезнуть");
    }

    #[tokio::test]
    async fn deleting_moderator_keeps_posts() {
        let db = prepared().await;
        db.run("DELETE FROM users WHERE id = 2").await;
        let t = db.table("SELECT id, moderator_id FROM posts").await;
        assert_eq!(t.rows, [["1", "NULL"]], "пост должен остаться, а модератор стать пустым:\n{t}");
    }

    #[tokio::test]
    async fn buyer_cannot_be_deleted() {
        let db = prepared().await;
        let err = db.fails("DELETE FROM users WHERE id = 3").await.expect("пользователя с покупками удалять нельзя");
        assert!(err.code == "23001" || err.code == "23503", "ждали нарушение внешнего ключа, а база сказала: {} ({})", err.message, err.code);
        assert_eq!(cell(&db, "SELECT count(*) FROM purchases").await, "1", "покупки должны уцелеть");
    }
}
