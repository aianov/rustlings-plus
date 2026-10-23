fn expand() -> &'static str {
    "ALTER TABLE posts ADD COLUMN body text;
     UPDATE posts SET body = text;
     CREATE FUNCTION posts_sync_text_body() RETURNS trigger LANGUAGE plpgsql AS $$
     BEGIN
         IF TG_OP = 'INSERT' THEN
             IF NEW.body IS NULL THEN NEW.body := NEW.text; END IF;
             IF NEW.text IS NULL THEN NEW.text := NEW.body; END IF;
         ELSIF NEW.text IS DISTINCT FROM OLD.text THEN
             NEW.body := NEW.text;
         ELSIF NEW.body IS DISTINCT FROM OLD.body THEN
             NEW.text := NEW.body;
         END IF;
         RETURN NEW;
     END $$;
     CREATE TRIGGER posts_sync_text_body BEFORE INSERT OR UPDATE ON posts FOR EACH ROW EXECUTE FUNCTION posts_sync_text_body()"
}

fn contract() -> &'static str {
    "DROP TRIGGER posts_sync_text_body ON posts;
     DROP FUNCTION posts_sync_text_body();
     ALTER TABLE posts DROP COLUMN text;
     ALTER TABLE posts ALTER COLUMN body SET NOT NULL"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const BASE: &str = "
        CREATE TABLE posts (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, author_id bigint NOT NULL, text text NOT NULL);
        INSERT INTO posts (author_id, text) SELECT 1, 'старый ' || g FROM generate_series(1, 100) g;
    ";

    async fn expanded() -> TestDb {
        let db = TestDb::with_sql(BASE).await;
        db.run(expand()).await;
        db
    }

    #[tokio::test]
    async fn expand_fills_existing_rows() {
        let db = expanded().await;
        let t = db.table("SELECT count(*) FROM posts WHERE body IS DISTINCT FROM text").await;
        assert_eq!(t.cell(0, 0), "0", "у существующих строк body должен совпадать с text");
    }

    #[tokio::test]
    async fn old_application_keeps_working() {
        let db = expanded().await;
        db.run("INSERT INTO posts (author_id, text) VALUES (1, 'от старого')").await;
        let t = db.table("SELECT body FROM posts WHERE text = 'от старого'").await;
        assert_eq!(t.col("body"), vec!["от старого"], "запись старой версии (только text) должна появиться в body");
        db.run("UPDATE posts SET text = 'правка старой версии' WHERE id = 1").await;
        let t = db.table("SELECT body FROM posts WHERE id = 1").await;
        assert_eq!(t.col("body"), vec!["правка старой версии"], "обновление text старой версией должно дойти до body");
    }

    #[tokio::test]
    async fn new_application_works_at_the_same_time() {
        let db = expanded().await;
        db.run("INSERT INTO posts (author_id, body) VALUES (1, 'от нового')").await;
        let t = db.table("SELECT text FROM posts WHERE body = 'от нового'").await;
        assert_eq!(t.col("text"), vec!["от нового"], "запись новой версии (только body) должна появиться в text, иначе старая версия её не увидит");
        db.run("UPDATE posts SET body = 'правка новой версии' WHERE id = 2").await;
        let t = db.table("SELECT text FROM posts WHERE id = 2").await;
        assert_eq!(t.col("text"), vec!["правка новой версии"], "обновление body новой версией должно дойти до text");
    }

    #[tokio::test]
    async fn contract_removes_the_scaffolding() {
        let db = expanded().await;
        db.run("INSERT INTO posts (author_id, text) VALUES (1, 'последняя запись старой версии')").await;
        db.run(contract()).await;
        let t = db.table("SELECT count(*) FROM information_schema.columns WHERE table_schema = current_schema() AND table_name = 'posts' AND column_name = 'text'").await;
        assert_eq!(t.cell(0, 0), "0", "столбец text должен быть удалён");
        let t = db.table("SELECT count(*) FROM pg_trigger WHERE tgrelid = 'posts'::regclass AND NOT tgisinternal").await;
        assert_eq!(t.cell(0, 0), "0", "триггер должен быть удалён");
        let t = db.table("SELECT count(*) FROM pg_proc WHERE proname LIKE 'posts_sync%' AND pronamespace = (SELECT oid FROM pg_namespace WHERE nspname = current_schema())").await;
        assert_eq!(t.cell(0, 0), "0", "функция триггера должна быть удалена");
        let t = db.table("SELECT is_nullable FROM information_schema.columns WHERE table_schema = current_schema() AND table_name = 'posts' AND column_name = 'body'").await;
        assert_eq!(t.cell(0, 0), "NO", "после contract столбец body обязателен");
        let t = db.table("SELECT count(*) FROM posts WHERE body = 'последняя запись старой версии'").await;
        assert_eq!(t.cell(0, 0), "1", "данные, записанные в промежутке, должны сохраниться в body");
    }
}
