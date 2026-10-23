// Квиз: релиз «посты 2.0». В таблице `posts` нужно сделать три изменения: переименовать `text` в `body`,
// сделать `author_id` обязательным (NOT NULL) и добавить внешний ключ на `users`. Всё это на живой
// таблице, пока выкладывается новая версия приложения, и старая версия (она читает и пишет только `text`)
// не должна заметить ни одной ошибки. Ниже лежит «прямолинейный» вариант: он мгновенный и роняет
// старую версию.
// Тест выполняет команды по очереди и после каждой работает «как старая версия» (`text`) и, пока
// `body` существует, «как новая версия» (`body`). Затем проверит правила безопасных изменений
// и итоговое состояние: `body` с теми же данными, нет `text`, `author_id` NOT NULL, внешний ключ проверен,
// временных ограничений, триггеров и функций не осталось.
// TODO: Верни команды `statements` в нужном порядке. Старой версии, что бы ни происходило, должно
// быть хорошо до того момента, как команды начнут убирать леса (триггер, функцию, старый столбец).

fn statements() -> Vec<&'static str> {
    vec![
        "ALTER TABLE posts RENAME COLUMN text TO body",
        "ALTER TABLE posts ALTER COLUMN author_id SET NOT NULL",
        "ALTER TABLE posts ADD CONSTRAINT posts_author_fk FOREIGN KEY (author_id) REFERENCES users (id)",
    ]
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SETUP: &str = "
        CREATE TABLE users (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, username text NOT NULL);
        INSERT INTO users (username) VALUES ('anna'), ('boris');
        CREATE TABLE posts (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, author_id bigint, text text NOT NULL);
        INSERT INTO posts (author_id, text) SELECT 1 + g % 2, 'пост ' || g FROM generate_series(1, 20000) g;
        CREATE TABLE original AS SELECT id, text FROM posts;
    ";

    async fn count(db: &TestDb, sql: &str) -> i64 {
        db.table(sql).await.cell(0, 0).parse().unwrap()
    }

    async fn column_exists(db: &TestDb, name: &str) -> bool {
        count(db, &format!("SELECT count(*) FROM information_schema.columns WHERE table_schema = current_schema() AND table_name = 'posts' AND column_name = '{name}'")).await == 1
    }

    async fn body_ready(db: &TestDb) -> bool {
        column_exists(db, "body").await && count(db, "SELECT count(*) FROM pg_trigger WHERE tgrelid = 'posts'::regclass AND NOT tgisinternal").await > 0
    }

    async fn old_version(db: &TestDb, tag: &str) -> Result<(), String> {
        let id: i64 = sqlx::query_scalar("INSERT INTO posts (author_id, text) VALUES (1, $1) RETURNING id")
            .bind(format!("старая {tag}"))
            .fetch_one(&db.pool)
            .await
            .map_err(|e| format!("вставка старой версии: {e}"))?;
        let text: String = sqlx::query_scalar("SELECT text FROM posts WHERE id = $1").bind(id).fetch_one(&db.pool).await.map_err(|e| format!("чтение старой версии: {e}"))?;
        sqlx::query("UPDATE posts SET text = $1 WHERE id = $2").bind(format!("{text} правка")).bind(id).execute(&db.pool).await.map_err(|e| format!("обновление старой версии: {e}"))?;
        Ok(())
    }

    async fn new_version(db: &TestDb, tag: &str) -> Result<(), String> {
        let id: i64 = sqlx::query_scalar("INSERT INTO posts (author_id, body) VALUES (1, $1) RETURNING id")
            .bind(format!("новая {tag}"))
            .fetch_one(&db.pool)
            .await
            .map_err(|e| format!("вставка новой версии: {e}"))?;
        let body: String = sqlx::query_scalar("SELECT body FROM posts WHERE id = $1").bind(id).fetch_one(&db.pool).await.map_err(|e| format!("чтение новой версии: {e}"))?;
        sqlx::query("UPDATE posts SET body = $1 WHERE id = $2").bind(format!("{body} правка")).bind(id).execute(&db.pool).await.map_err(|e| format!("обновление новой версии: {e}"))?;
        Ok(())
    }

    #[tokio::test]
    async fn both_versions_work_at_every_step() {
        let db = TestDb::with_sql(SETUP).await;
        let mut in_contract = false;
        for (i, stmt) in statements().into_iter().enumerate() {
            let low = stmt.to_lowercase();
            if low.contains("drop trigger") || low.contains("drop function") || low.contains("drop column") {
                in_contract = true;
            }
            if low.contains("set not null") {
                let validated = count(&db, "SELECT count(*) FROM pg_constraint WHERE conrelid = 'posts'::regclass AND contype = 'c' AND convalidated").await;
                assert!(validated >= 1, "шаг {i}: перед SET NOT NULL нужно уже проверенное ограничение CHECK (NOT VALID, затем VALIDATE), иначе SET NOT NULL читает всю таблицу под жёсткой блокировкой");
            }
            db.run(stmt).await;
            if low.contains("foreign key") {
                let validated = count(&db, "SELECT count(*) FROM pg_constraint WHERE conrelid = 'posts'::regclass AND contype = 'f' AND convalidated").await;
                assert_eq!(validated, 0, "шаг {i}: внешний ключ должен появиться как NOT VALID, а проверяться отдельной командой VALIDATE CONSTRAINT");
            }
            if !in_contract {
                if let Err(e) = old_version(&db, &format!("шаг {i}")).await {
                    panic!("после шага {i} (`{stmt}`) старая версия приложения получила ошибку: {e}");
                }
                if body_ready(&db).await {
                    if let Err(e) = new_version(&db, &format!("шаг {i}")).await {
                        panic!("после шага {i} (`{stmt}`) новая версия приложения получила ошибку: {e}");
                    }
                    let drift = count(&db, "SELECT count(*) FROM posts WHERE body IS NOT NULL AND body IS DISTINCT FROM text").await;
                    assert_eq!(drift, 0, "после шага {i} в {drift} строках body и text разошлись: пока живут обе версии, они должны совпадать");
                }
            } else if let Err(e) = new_version(&db, &format!("шаг {i}")).await {
                panic!("после шага {i} (`{stmt}`) новая версия приложения получила ошибку: {e}. Пока идёт сужение, между удалением триггера и столбца text новая версия не должна проваливаться (подсказка: можно выполнить несколько команд одним шагом)");
            }
        }
        assert!(in_contract, "в плане нет шага «убрать старое» (триггер, функция, столбец text)");
    }

    #[tokio::test]
    async fn final_state_is_clean() {
        let db = TestDb::with_sql(SETUP).await;
        for stmt in statements() {
            db.run(stmt).await;
        }
        assert!(column_exists(&db, "body").await, "должен быть столбец body");
        assert!(!column_exists(&db, "text").await, "столбец text должен быть удалён");
        assert_eq!(count(&db, "SELECT count(*) FROM posts WHERE body IS NULL").await, 0, "у некоторых строк body пуст: записи старой версии, сделанные до того, как заработал триггер, остались незаполненными. Сначала триггер, потом перенос данных");
        let lost = count(&db, "SELECT count(*) FROM original o LEFT JOIN posts p ON p.id = o.id WHERE p.body IS DISTINCT FROM o.text").await;
        assert_eq!(lost, 0, "у {lost} старых постов body не совпадает с прежним text");
        let t = db.table("SELECT is_nullable FROM information_schema.columns WHERE table_schema = current_schema() AND table_name = 'posts' AND column_name = 'author_id'").await;
        assert_eq!(t.cell(0, 0), "NO", "author_id должен быть NOT NULL");
        let fk = count(&db, "SELECT count(*) FROM pg_constraint WHERE conrelid = 'posts'::regclass AND contype = 'f' AND convalidated").await;
        assert_eq!(fk, 1, "должен быть один проверенный внешний ключ");
        let checks = count(&db, "SELECT count(*) FROM pg_constraint WHERE conrelid = 'posts'::regclass AND contype = 'c'").await;
        assert_eq!(checks, 0, "временные ограничения CHECK должны быть удалены");
        let triggers = count(&db, "SELECT count(*) FROM pg_trigger WHERE tgrelid = 'posts'::regclass AND NOT tgisinternal").await;
        assert_eq!(triggers, 0, "триггер синхронизации должен быть удалён");
        let res = db.fails("INSERT INTO posts (author_id, body) VALUES (999, 'призрак')").await;
        assert_eq!(res.map(|e| e.code), Some("23503".to_string()), "внешний ключ должен отвергать несуществующего автора");
    }
}
