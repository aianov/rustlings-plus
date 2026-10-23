// Ночная чистка: все черновики (posts.is_draft = true) удаляются. Автору утром придёт
// письмо «ваши черновики удалены», и для письма нужны id удалённого поста и его автор. Читать
// черновики отдельным запросом перед удалением нельзя: за это время появятся новые.
// TODO: Напиши запрос, который удаляет черновики и возвращает id и author_id удалённых строк.

fn purge_drafts() -> &'static str {
    "DELETE FROM posts WHERE is_draft"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");


    #[tokio::test]
    async fn drafts_are_deleted_and_reported() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(purge_drafts()).await.sorted();
        assert_eq!(t.columns, ["id", "author_id"], "нужны столбцы id и author_id");
        assert_eq!(t.rows, [["4", "3"], ["5", "2"]], "удалены должны быть черновики 4 и 5:\n{t}");
        let left = db.table("SELECT id FROM posts ORDER BY id").await;
        assert_eq!(left.column(0), ["1", "2", "3", "6"], "остальные посты должны остаться:\n{left}");
    }
}
