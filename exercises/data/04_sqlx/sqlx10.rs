// Страница «Люди» не открывается совсем: ошибка на первой же загрузке списка. Но на тестовой
// базе, где у всех написано «о себе», всё работало. Причина в данных: у части пользователей
// поле bio пустое. Сигнатуры оставь как есть. Строка под ником должна читаться
// «ник: о себе», а для пустого bio «ник: (о себе ничего не написано)».
// TODO: Почини загрузку и вывод.

use sqlx::PgPool;

#[derive(Debug, sqlx::FromRow)]
struct Profile {
    username: String,
    bio: String,
}

async fn profiles(pool: &PgPool) -> Result<Vec<Profile>, sqlx::Error> {
    sqlx::query_as::<_, Profile>("SELECT username, bio FROM users ORDER BY id")
        .fetch_all(pool)
        .await
}

fn bio_line(p: &Profile) -> String {
    format!("{}: {}", p.username, p.bio)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn everyone_loads() {
        let db = TestDb::with_sql(SEED).await;
        let list = profiles(&db.pool).await.expect("профили с пустым bio тоже должны загружаться");
        assert_eq!(list.len(), 6, "в базе шесть пользователей");
    }

    #[tokio::test]
    async fn lines_handle_both_cases() {
        let db = TestDb::with_sql(SEED).await;
        let list = profiles(&db.pool).await.unwrap();
        let lines: Vec<String> = list.iter().map(bio_line).collect();
        assert_eq!(lines[0], "alice: Пишу про Rust", "у alice bio заполнено");
        assert_eq!(lines[1], "bob: (о себе ничего не написано)", "у bob bio пустое");
        assert_eq!(lines[3], "dave: (о себе ничего не написано)", "у dave bio пустое");
    }
}
