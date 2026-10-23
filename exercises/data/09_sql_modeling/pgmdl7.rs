// У поста есть статус: draft (по умолчанию), published или archived. Другие значения база
// принимать не должна, а через месяц понадобится ещё одно значение, banned, и добавить его надо
// будет без пересоздания таблицы и потери данных. Таблица posts: id (generated identity),
// body (обязателен), status (обязателен). Функция `add_banned` возвращает скрипт, который
// разрешает ещё и banned в уже работающей базе.
// Способ на твой выбор: text с CHECK, enum-тип или справочник статусов.
// TODO: Напиши схему и скрипт расширения.

fn schema() -> &'static str {
    "CREATE TABLE posts (
         id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
         body text NOT NULL,
         status text NOT NULL DEFAULT 'draft'
     )"
}

fn add_banned() -> &'static str {
    "SELECT 1"
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
