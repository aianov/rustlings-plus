use sqlx::{PgPool, QueryBuilder};

#[derive(Default)]
struct PostFilter {
    author: Option<String>,
    tag: Option<String>,
    text: Option<String>,
    min_likes: Option<i32>,
    published_only: bool,
}

async fn search_posts(pool: &PgPool, f: &PostFilter) -> Result<Vec<i64>, sqlx::Error> {
    let mut qb = QueryBuilder::new("SELECT p.id FROM posts p WHERE TRUE");
    if let Some(author) = &f.author {
        qb.push(" AND EXISTS (SELECT 1 FROM users u WHERE u.id = p.author_id AND u.username = ");
        qb.push_bind(author);
        qb.push(")");
    }
    if let Some(tag) = &f.tag {
        qb.push(" AND EXISTS (SELECT 1 FROM post_tags pt JOIN tags t ON t.id = pt.tag_id WHERE pt.post_id = p.id AND t.name = ");
        qb.push_bind(tag);
        qb.push(")");
    }
    if let Some(text) = &f.text {
        qb.push(" AND p.body ILIKE ");
        qb.push_bind(format!("%{text}%"));
    }
    if let Some(min) = f.min_likes {
        qb.push(" AND p.likes_count >= ");
        qb.push_bind(min);
    }
    if f.published_only {
        qb.push(" AND p.status = 'published'");
    }
    qb.push(" ORDER BY p.id");
    qb.build_query_scalar::<i64>().fetch_all(pool).await
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    async fn ids(db: &TestDb, f: PostFilter) -> Vec<i64> {
        search_posts(&db.pool, &f).await.expect("поиск должен выполниться")
    }

    #[tokio::test]
    async fn no_filters_means_everything() {
        let db = TestDb::with_sql(SEED).await;
        assert_eq!(ids(&db, PostFilter::default()).await, vec![1, 2, 3, 4, 5, 6, 7, 8], "без фильтров должны быть все посты");
        assert_eq!(ids(&db, PostFilter { published_only: true, ..Default::default() }).await, vec![1, 2, 3, 5, 6, 8], "только опубликованные");
    }

    #[tokio::test]
    async fn single_filters_narrow_the_result() {
        let db = TestDb::with_sql(SEED).await;
        assert_eq!(ids(&db, PostFilter { author: Some("anna".into()), ..Default::default() }).await, vec![1, 2, 8], "посты anna");
        assert_eq!(ids(&db, PostFilter { tag: Some("rust".into()), ..Default::default() }).await, vec![1, 2, 7], "посты с тегом rust");
        assert_eq!(ids(&db, PostFilter { text: Some("rust".into()), ..Default::default() }).await, vec![1, 2, 7], "поиск по тексту без учёта регистра");
        assert_eq!(ids(&db, PostFilter { min_likes: Some(3), ..Default::default() }).await, vec![1, 6], "не меньше трёх лайков");
    }

    #[tokio::test]
    async fn filters_are_combined_with_and() {
        let db = TestDb::with_sql(SEED).await;
        let f = PostFilter { author: Some("anna".into()), tag: Some("rust".into()), ..Default::default() };
        assert_eq!(ids(&db, f).await, vec![1, 2], "anna и тег rust");
        let f = PostFilter { tag: Some("rust".into()), published_only: true, ..Default::default() };
        assert_eq!(ids(&db, f).await, vec![1, 2], "тег rust, но не черновик 7");
        let f = PostFilter { min_likes: Some(2), published_only: true, text: Some("пост".into()), ..Default::default() };
        assert_eq!(ids(&db, f).await, vec![1, 8], "три условия сразу: от двух лайков, опубликован, в тексте «пост»");
        let f = PostFilter { author: Some("boris".into()), min_likes: Some(1), ..Default::default() };
        assert_eq!(ids(&db, f).await, Vec::<i64>::new(), "у boris нет постов с лайками");
    }

    #[tokio::test]
    async fn client_values_never_become_sql() {
        let db = TestDb::with_sql(SEED).await;
        let f = PostFilter { author: Some("anna' OR '1'='1".into()), ..Default::default() };
        assert_eq!(ids(&db, f).await, Vec::<i64>::new(), "строка вместо ника должна искаться как ник, а не как кусок SQL");
        let f = PostFilter { text: Some("'; DROP TABLE posts; --".into()), ..Default::default() };
        assert_eq!(ids(&db, f).await, Vec::<i64>::new(), "такого текста в постах нет");
        let t = db.table("SELECT count(*) FROM posts").await;
        assert_eq!(t.cell(0, 0), "8", "таблица posts должна остаться на месте");
    }
}
