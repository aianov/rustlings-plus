fn video_stats() -> &'static str {
    "CREATE TABLE video_stats (
        id bigint PRIMARY KEY,
        views bigint NOT NULL,
        title text NOT NULL,
        rating numeric(3, 2) NOT NULL
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

    #[tokio::test]
    async fn huge_view_counter() {
        let db = TestDb::new().await;
        db.run(video_stats()).await;
        db.run("INSERT INTO video_stats VALUES (1, 5000000000, 'viral', 4.5)").await;
        assert_eq!(cell(&db, "SELECT views FROM video_stats").await, "5000000000", "счётчик просмотров должен вмещать пять миллиардов");
    }

    #[tokio::test]
    async fn long_titles() {
        let db = TestDb::new().await;
        db.run(video_stats()).await;
        db.run("INSERT INTO video_stats VALUES (1, 1, repeat('x', 300), 4.5)").await;
        assert_eq!(cell(&db, "SELECT length(title) FROM video_stats").await, "300", "длинное название должно сохраниться целиком");
    }

    #[tokio::test]
    async fn column_types() {
        let db = TestDb::new().await;
        db.run(video_stats()).await;
        let t = db
            .table(
                "SELECT column_name, data_type FROM information_schema.columns
                 WHERE table_schema = current_schema() AND table_name = 'video_stats' AND column_name IN ('id', 'title')
                 ORDER BY column_name",
            )
            .await;
        assert_eq!(t.rows, [["id", "bigint"], ["title", "text"]], "идентификатор тоже растёт, ему нужен bigint, а название без предела длины это text:\n{t}");
    }

    #[tokio::test]
    async fn ratings_add_up_exactly() {
        let db = TestDb::new().await;
        db.run(video_stats()).await;
        db.run("INSERT INTO video_stats VALUES (1, 1, 'a', 0.1), (2, 1, 'b', 0.2)").await;
        assert_eq!(
            cell(&db, "SELECT sum(rating) = 0.3 FROM video_stats").await,
            "true",
            "0.1 + 0.2 должно быть ровно 0.3: для оценок нужен точный десятичный тип, а не двоичная дробь"
        );
    }
}
