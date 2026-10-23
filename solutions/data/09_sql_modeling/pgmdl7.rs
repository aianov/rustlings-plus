fn schema() -> &'static str {
    "CREATE TABLE posts (
         id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
         body text NOT NULL,
         status text NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'published', 'archived'))
     )"
}

fn add_banned() -> &'static str {
    "ALTER TABLE posts DROP CONSTRAINT posts_status_check;
     ALTER TABLE posts ADD CONSTRAINT posts_status_check CHECK (status IN ('draft', 'published', 'archived', 'banned'))"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    async fn rejected(db: &TestDb, sql: &str, why: &str) {
        let err = db.fails(sql).await.unwrap_or_else(|| panic!("{why}: Postgres принял команду, а должен был отвергнуть:\n{sql}"));
        assert!(["23514", "22P02", "23503"].contains(&err.code.as_str()), "{why}: ждали отказ по CHECK, enum или внешнему ключу, а база сказала {} ({})", err.code, err.message);
    }

    #[tokio::test]
    async fn statuses_are_closed_then_extended() {
        let db = TestDb::new().await;
        db.run(schema()).await;
        db.run("INSERT INTO posts (body) VALUES ('a'); INSERT INTO posts (body, status) VALUES ('b', 'published'), ('c', 'archived')").await;
        assert_eq!(db.table("SELECT status::text FROM posts WHERE id = 1").await.cell(0, 0), "draft", "статус по умолчанию draft");
        rejected(&db, "INSERT INTO posts (body, status) VALUES ('d', 'deleted')", "неизвестный статус").await;
        rejected(&db, "INSERT INTO posts (body, status) VALUES ('d', 'banned')", "banned пока не разрешён").await;
        db.run(add_banned()).await;
        db.run("INSERT INTO posts (body, status) VALUES ('d', 'banned')").await;
        rejected(&db, "INSERT INTO posts (body, status) VALUES ('e', 'bogus')", "после расширения чужие значения всё ещё запрещены").await;
        assert_eq!(db.table("SELECT count(*) FROM posts").await.cell(0, 0), "4", "старые данные на месте, добавилась одна строка");
    }
}
