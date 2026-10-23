// Лайки и подписки с счётчиками под параллельной нагрузкой. Таблицы: users (followers_count,
// following_count), posts (likes_count), likes (user_id, post_id), follows (follower_id, followee_id).
// Счётчики должны всегда совпадать с настоящим числом строк в likes и follows.
//  - `like_post(user, post)`: ставит лайк, возвращает новое likes_count. Повторный лайк того же
//    человека не меняет ничего (идемпотентность).
//  - `unlike_post(user, post)`: снимает лайк, возвращает likes_count; если лайка не было, ничего не меняет.
//  - `follow(a, b)`: a подписывается на b; true, если подписка новая, false, если уже была. Растут
//    a.following_count и b.followers_count в той же транзакции.
//  - `unfollow(a, b)`: симметрично, true, если подписка была.
// Встречные подписки (a на b и b на a одновременно) не должны давать дедлок 40P01 или другие ошибки.
// TODO: Напиши четыре функции.

use sqlx::PgPool;

async fn like_post(pool: &PgPool, user_id: i64, post_id: i64) -> Result<i32, sqlx::Error> {
    todo!()
}

async fn unlike_post(pool: &PgPool, user_id: i64, post_id: i64) -> Result<i32, sqlx::Error> {
    todo!()
}

async fn follow(pool: &PgPool, a: i64, b: i64) -> Result<bool, sqlx::Error> {
    todo!()
}

async fn unfollow(pool: &PgPool, a: i64, b: i64) -> Result<bool, sqlx::Error> {
    todo!()
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SCHEMA: &str = "
        CREATE TABLE users (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, followers_count integer NOT NULL DEFAULT 0 CHECK (followers_count >= 0), following_count integer NOT NULL DEFAULT 0 CHECK (following_count >= 0));
        CREATE TABLE posts (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, likes_count integer NOT NULL DEFAULT 0 CHECK (likes_count >= 0));
        CREATE TABLE likes (user_id bigint NOT NULL REFERENCES users (id), post_id bigint NOT NULL REFERENCES posts (id), PRIMARY KEY (user_id, post_id));
        CREATE TABLE follows (follower_id bigint NOT NULL REFERENCES users (id), followee_id bigint NOT NULL REFERENCES users (id), PRIMARY KEY (follower_id, followee_id), CHECK (follower_id <> followee_id));
        INSERT INTO users SELECT FROM generate_series(1, 12);
        INSERT INTO posts DEFAULT VALUES; INSERT INTO posts DEFAULT VALUES;
    ";

    async fn invariant(db: &TestDb) {
        let t = db.table("SELECT p.id FROM posts p WHERE p.likes_count <> (SELECT count(*) FROM likes l WHERE l.post_id = p.id)").await;
        assert!(t.is_empty(), "likes_count разошёлся с таблицей likes у постов:\n{t}");
        let t = db.table("SELECT u.id FROM users u WHERE u.followers_count <> (SELECT count(*) FROM follows f WHERE f.followee_id = u.id) OR u.following_count <> (SELECT count(*) FROM follows f WHERE f.follower_id = u.id)").await;
        assert!(t.is_empty(), "счётчики подписок разошлись с таблицей follows у пользователей:\n{t}");
    }

    #[tokio::test]
    async fn double_clicks_count_once() {
        let db = TestDb::with_sql(SCHEMA).await;
        let mut handles = Vec::new();
        for _ in 0..12 {
            let pool = db.pool.clone();
            handles.push(tokio::spawn(async move { like_post(&pool, 3, 1).await }));
        }
        for h in handles {
            h.await.expect("задача упала").expect("like_post вернул ошибку");
        }
        assert_eq!(db.table("SELECT likes_count FROM posts WHERE id = 1").await.cell(0, 0), "1", "двенадцать лайков одного человека дают один");
        invariant(&db).await;
    }

    #[tokio::test]
    async fn many_likers_and_unlikers() {
        let db = TestDb::with_sql(SCHEMA).await;
        let mut handles = Vec::new();
        for user in 1..=12_i64 {
            let pool = db.pool.clone();
            handles.push(tokio::spawn(async move {
                like_post(&pool, user, 1).await?;
                if user % 3 == 0 {
                    unlike_post(&pool, user, 1).await?;
                    unlike_post(&pool, user, 1).await?;
                }
                like_post(&pool, user, 2).await
            }));
        }
        for h in handles {
            h.await.expect("задача упала").expect("операция вернула ошибку");
        }
        assert_eq!(db.table("SELECT likes_count FROM posts WHERE id = 1").await.cell(0, 0), "8", "двенадцать лайков минус четыре снятых");
        assert_eq!(db.table("SELECT likes_count FROM posts WHERE id = 2").await.cell(0, 0), "12", "на втором посте двенадцать");
        invariant(&db).await;
    }

    #[tokio::test]
    async fn crossing_follows_do_not_deadlock() {
        let db = TestDb::with_sql(SCHEMA).await;
        let mut handles = Vec::new();
        for round in 0..6_i64 {
            for (a, b) in [(1_i64, 2_i64), (2, 1), (1, 3), (3, 1), (2, 3), (3, 2)] {
                let pool = db.pool.clone();
                handles.push(tokio::spawn(async move { follow(&pool, a, b).await.map(|new| (round, new)) }));
            }
        }
        let mut new = 0;
        for h in handles {
            match h.await.expect("задача упала") {
                Ok((_, true)) => new += 1,
                Ok((_, false)) => {}
                Err(e) => panic!("follow вернул ошибку {e}: встречные подписки блокируют друг друга, блокировки пользователей нужно брать в одном порядке"),
            }
        }
        assert_eq!(new, 6, "разных подписок шесть, остальные 30 вызовов повторы и возвращают false");
        invariant(&db).await;
    }

    #[tokio::test]
    async fn unfollow_is_idempotent() {
        let db = TestDb::with_sql(SCHEMA).await;
        assert!(follow(&db.pool, 4, 5).await.expect("follow"), "первая подписка новая");
        assert!(!follow(&db.pool, 4, 5).await.expect("follow"), "повторная не новая");
        assert!(unfollow(&db.pool, 4, 5).await.expect("unfollow"), "подписка была");
        assert!(!unfollow(&db.pool, 4, 5).await.expect("unfollow"), "второй раз снимать нечего");
        invariant(&db).await;
        assert_eq!(db.table("SELECT followers_count FROM users WHERE id = 5").await.cell(0, 0), "0");
    }
}
