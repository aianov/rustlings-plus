// Вход в соцсеть выдаёт сессию: случайный идентификатор (uuid, его создаёт приложение, а не
// база) и срок жизни. Сессия живёт строго до expires_at: ровно в момент окончания она уже
// считается просроченной.
//  - `create_session`: сохранить новую сессию пользователя со сроком now + ttl и вернуть её;
//  - `find_live_session`: найти сессию по uuid, если на момент `now` она ещё жива. Текущее
//    время приходит параметром, чтобы тест мог подвинуть часы.
// Столбец expires_at в базе имеет тип timestamptz.
// TODO: Реализуй обе функции с подходящими типами Rust.

use chrono::{DateTime, Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, sqlx::FromRow)]
struct Session {
    id: Uuid,
    user_id: i64,
    expires_at: DateTime<Utc>,
}

async fn create_session(pool: &PgPool, user_id: i64, ttl: Duration) -> Result<Session, sqlx::Error> {
    todo!()
}

async fn find_live_session(pool: &PgPool, id: Uuid, now: DateTime<Utc>) -> Result<Option<Session>, sqlx::Error> {
    todo!()
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn session_is_created_and_returned() {
        let db = TestDb::with_sql(SEED).await;
        let before = Utc::now();
        let s = create_session(&db.pool, 2, Duration::hours(1)).await.expect("сессия должна создаться");
        let after = Utc::now();
        assert!(!s.id.is_nil(), "у сессии должен быть настоящий uuid");
        assert_eq!(s.user_id, 2, "сессия принадлежит пользователю 2");
        assert!(s.expires_at >= before + Duration::hours(1) && s.expires_at <= after + Duration::hours(1), "срок должен быть now + ttl, а получилось {}", s.expires_at);
        let stored = db.table("SELECT count(*) FROM sessions").await;
        assert_eq!(stored.cell(0, 0), "1", "сессия должна быть в таблице");
    }

    #[tokio::test]
    async fn alive_until_expiry_and_not_after() {
        let db = TestDb::with_sql(SEED).await;
        let s = create_session(&db.pool, 1, Duration::minutes(30)).await.unwrap();
        let alive = find_live_session(&db.pool, s.id, s.expires_at - Duration::seconds(1)).await.unwrap();
        assert_eq!(alive.map(|x| x.id), Some(s.id), "за секунду до срока сессия жива");
        let dead = find_live_session(&db.pool, s.id, s.expires_at).await.unwrap();
        assert!(dead.is_none(), "ровно в момент expires_at сессия уже просрочена");
        let dead = find_live_session(&db.pool, s.id, s.expires_at + Duration::hours(2)).await.unwrap();
        assert!(dead.is_none(), "через два часа после срока сессия мертва");
    }

    #[tokio::test]
    async fn unknown_session_is_none() {
        let db = TestDb::with_sql(SEED).await;
        let got = find_live_session(&db.pool, Uuid::new_v4(), Utc::now()).await.unwrap();
        assert!(got.is_none(), "неизвестный uuid: None");
    }
}
