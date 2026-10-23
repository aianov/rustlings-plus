// Два пакетных запроса. Первый: из старой системы пришли три новых пользователя (gina, hank,
// ivy, почты вида имя@example.com), и их надо добавить одним запросом, а не тремя.
// Второй: забаненных пользователей нужно скопировать в таблицу users_archive (там только ник и
// почта), не выгружая их в приложение и не набирая руками.
// TODO: Напиши оба запроса.

fn import_users() -> &'static str {
    "SELECT 1"
}

fn archive_banned() -> &'static str {
    "SELECT 1"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");


    #[tokio::test]
    async fn three_users_in_one_statement() {
        let db = TestDb::with_sql(SEED).await;
        db.run(import_users()).await;
        let t = db.table("SELECT username, email FROM users WHERE id > 6 ORDER BY id").await;
        assert_eq!(
            t.rows,
            [["gina", "gina@example.com"], ["hank", "hank@example.com"], ["ivy", "ivy@example.com"]],
            "должны добавиться три пользователя:\n{t}"
        );
    }

    #[tokio::test]
    async fn insert_from_select() {
        let db = TestDb::with_sql(SEED).await;
        db.run(archive_banned()).await;
        let t = db.table("SELECT username, email FROM users_archive").await;
        assert_eq!(t.rows, [["carol", "carol@mail.org"]], "в архив должен попасть только забаненный carol:\n{t}");
        assert_eq!(db.table("SELECT id FROM users").await.len(), 6, "из users ничего удаляться не должно");
    }

    #[tokio::test]
    async fn import_is_a_single_insert() {
        let text = import_users().to_lowercase();
        assert_eq!(text.matches("insert").count(), 1, "пакет должен быть одним INSERT с тремя наборами значений, а не тремя командами подряд");
    }
}
