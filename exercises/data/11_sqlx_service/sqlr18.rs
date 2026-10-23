// Репозиторий не должен выпускать наружу строки базы: статус там текст, число лайков знаковое,
// и ничто не мешает записать туда мусор. Снаружи нужны доменные типы. Между ними граница:
// преобразование `PostRow -> Post`, которое проверяет данные и отказывает, если в базе
// лежит неправильное (неизвестный статус, отрицательные лайки). Отказ это ошибка `Corrupt`, а не
// паника и не молчаливая подмена. Запросы возвращают автора по имени, а не по id.
// TODO: Допиши преобразование и оба метода репозитория.

use sqlx::PgPool;
use std::future::Future;

#[derive(Debug, Clone, Copy, PartialEq)]
struct PostId(i64);

#[derive(Debug, Clone, Copy, PartialEq)]
enum PostStatus {
    Draft,
    Published,
}

#[derive(Debug, PartialEq)]
struct Post {
    id: PostId,
    author: String,
    body: String,
    status: PostStatus,
    likes: u32,
}

#[derive(Debug)]
enum RepoError {
    Corrupt(String),
    Db(sqlx::Error),
}

impl From<sqlx::Error> for RepoError {
    fn from(e: sqlx::Error) -> RepoError {
        RepoError::Db(e)
    }
}

#[derive(sqlx::FromRow)]
struct PostRow {
    id: i64,
    author: String,
    body: String,
    status: String,
    likes_count: i32,
}

trait PostRepo {
    fn get(&self, id: PostId) -> impl Future<Output = Result<Option<Post>, RepoError>> + Send;
    fn by_author(&self, username: &str) -> impl Future<Output = Result<Vec<Post>, RepoError>> + Send;
}

struct PgPostRepo {
    pool: PgPool,
}

const GET_SQL: &str = "SELECT p.id, u.username AS author, p.body, p.status, p.likes_count FROM posts p JOIN users u ON u.id = p.author_id WHERE p.id = $1";
const BY_AUTHOR_SQL: &str = "SELECT p.id, u.username AS author, p.body, p.status, p.likes_count FROM posts p JOIN users u ON u.id = p.author_id WHERE u.username = $1 ORDER BY p.created_at DESC, p.id DESC";

impl TryFrom<PostRow> for Post {
    type Error = RepoError;

    fn try_from(row: PostRow) -> Result<Post, RepoError> {
        let status = if row.status == "draft" { PostStatus::Draft } else { PostStatus::Published };
        Ok(Post { id: PostId(row.id), author: row.author, body: row.body, status, likes: row.likes_count as u32 })
    }
}

impl PostRepo for PgPostRepo {
    async fn get(&self, id: PostId) -> Result<Option<Post>, RepoError> {
        todo!()
    }

    async fn by_author(&self, username: &str) -> Result<Vec<Post>, RepoError> {
        todo!()
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    fn repo(db: &TestDb) -> PgPostRepo {
        PgPostRepo { pool: db.pool.clone() }
    }

    #[tokio::test]
    async fn reads_domain_posts() {
        let db = TestDb::with_sql(SEED).await;
        let r = repo(&db);
        let post = r.get(PostId(1)).await.expect("чтение поста 1").expect("пост 1 есть");
        assert_eq!(
            post,
            Post { id: PostId(1), author: "anna".to_string(), body: "Первый пост про Rust".to_string(), status: PostStatus::Published, likes: 3 },
            "пост 1 как доменный тип"
        );
        let draft = r.get(PostId(4)).await.expect("чтение черновика").expect("черновик есть");
        assert_eq!(draft.status, PostStatus::Draft, "статус draft должен стать PostStatus::Draft");
        assert!(r.get(PostId(99)).await.expect("чтение отсутствующего").is_none(), "несуществующий пост это None, а не ошибка");
        let ids: Vec<i64> = r.by_author("anna").await.expect("посты anna").iter().map(|p| p.id.0).collect();
        assert_eq!(ids, vec![2, 8, 1], "посты автора идут от новых к старым");
    }

    #[tokio::test]
    async fn broken_rows_are_reported_not_hidden() {
        let db = TestDb::with_sql(SEED).await;
        db.run("UPDATE posts SET status = 'archived' WHERE id = 3; UPDATE posts SET likes_count = -5 WHERE id = 5").await;
        let r = repo(&db);
        match r.get(PostId(3)).await {
            Err(RepoError::Corrupt(m)) => assert!(m.contains('3'), "в сообщении должен быть id поста: {m}"),
            Err(RepoError::Db(e)) => panic!("это не ошибка базы: {e}"),
            Ok(p) => panic!("статус archived неизвестен, а чтение вернуло {p:?}"),
        }
        match r.get(PostId(5)).await {
            Err(RepoError::Corrupt(_)) => {}
            Err(RepoError::Db(e)) => panic!("это не ошибка базы: {e}"),
            Ok(p) => panic!("отрицательное число лайков принято как {p:?}"),
        }
        assert!(matches!(r.by_author("boris").await, Err(RepoError::Corrupt(_))), "в списке постов boris есть испорченный, весь список обязан сообщить об ошибке");
    }
}
