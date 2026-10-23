// Предыдущий поиск вычисляет `to_tsvector` для каждой строки на каждый запрос, то есть читает и
// разбирает все посты. Вектор надо хранить: отдельный столбец `search`, который база сама
// поддерживает в актуальном состоянии (вставки и правки ничего не должны знать про поиск), и GIN-индекс
// на нём. Заголовок важнее текста: слова из заголовка помечаются весом A, из текста B.
// TODO: Верни скрипт `migrate`: он добавляет в `posts` (id, title, body) столбец `search` и индекс.

fn migrate() -> &'static str {
    "SELECT 1"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;


    async fn plan_text(db: &TestDb, sql: &str) -> String {
        db.table(&format!("EXPLAIN {sql}")).await.column(0).join("\n")
    }

    const SEED: &str = "
        CREATE TABLE posts (id bigint PRIMARY KEY, title text NOT NULL, body text NOT NULL);
        INSERT INTO posts
            SELECT g, 'Заметка ' || g, 'обычный текст про погоду и кофе номер ' || g || CASE WHEN g % 1000 = 0 THEN ' клавикорд' ELSE '' END
            FROM generate_series(1, 30000) g;
        INSERT INTO posts VALUES (30001, 'Мой клавесин', 'пост без редких слов в теле');
        ANALYZE posts;
    ";
    const FIND_BODY: &str = "SELECT id FROM posts WHERE search @@ websearch_to_tsquery('russian', 'клавикорд') ORDER BY id";

    #[tokio::test]
    async fn column_is_maintained_by_the_database() {
        let db = TestDb::with_sql(SEED).await;
        db.run(migrate()).await;
        let t = db.table("SELECT data_type, is_generated FROM information_schema.columns WHERE table_schema = current_schema() AND table_name = 'posts' AND column_name = 'search'").await;
        assert_eq!(t.rows, vec![vec!["tsvector", "ALWAYS"]], "search должен быть столбцом tsvector, вычисляемым базой (GENERATED ALWAYS):\n{t}");
    }

    #[tokio::test]
    async fn search_finds_title_and_body_with_weights() {
        let db = TestDb::with_sql(SEED).await;
        db.run(migrate()).await;
        let t = db.table(FIND_BODY).await;
        assert_eq!(t.len(), 30, "слово «клавикорд» в тексте у 30 постов (каждый тысячный):\n{t}");
        assert_eq!(db.table("SELECT id FROM posts WHERE search @@ websearch_to_tsquery('russian', 'клавесин')").await.column(0), vec!["30001"], "слово из заголовка тоже ищется");
        assert_eq!(db.table("SELECT id FROM posts WHERE search @@ to_tsquery('russian', 'клавесин:A')").await.column(0), vec!["30001"], "слова заголовка должны иметь вес A");
        assert_eq!(db.table("SELECT count(*) FROM posts WHERE search @@ to_tsquery('russian', 'клавикорд:B')").await.cell(0, 0), "30", "слова текста должны иметь вес B");
        assert_eq!(db.table("SELECT count(*) FROM posts WHERE search @@ to_tsquery('russian', 'клавикорд:A')").await.cell(0, 0), "0", "слова текста не должны получить вес заголовка");
    }

    #[tokio::test]
    async fn search_goes_through_the_index() {
        let db = TestDb::with_sql(SEED).await;
        db.run(migrate()).await;
        db.run("ANALYZE posts").await;
        let plan = plan_text(&db, FIND_BODY).await;
        assert!(plan.contains("Bitmap Index Scan") && !plan.contains("Seq Scan on posts"), "поиск должен идти по GIN-индексу, а план такой:\n{plan}");
    }

    #[tokio::test]
    async fn new_and_edited_posts_are_searchable_immediately() {
        let db = TestDb::with_sql(SEED).await;
        db.run(migrate()).await;
        db.run("INSERT INTO posts VALUES (30002, 'Новость', 'опять клавикорд, теперь свежий')").await;
        assert!(db.table(FIND_BODY).await.column(0).contains(&"30002"), "вставленный пост должен находиться без дополнительных действий");
        db.run("UPDATE posts SET body = 'теперь про гитару' WHERE id = 1000").await;
        assert!(!db.table(FIND_BODY).await.column(0).contains(&"1000"), "после правки текста пост со старым словом находиться не должен");
        assert_eq!(db.table("SELECT id FROM posts WHERE search @@ websearch_to_tsquery('russian', 'гитара')").await.column(0), vec!["1000"], "а новое слово находится");
    }
}
