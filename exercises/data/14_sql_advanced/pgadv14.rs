// В таблице профилей есть `updated_at`, и его должна выставлять база: приложения-писатели разные,
// и каждое забывает. Кто-то уже написал триггер, но после его появления любая правка профиля падает
// с «stack depth limit exceeded». Плюс хочется, чтобы «пустая» правка (записали то же самое значение)
// не меняла `updated_at`: иначе клиенты зря перекачивают профили.
// TODO: Почини `trigger_sql`: после изменения строки `updated_at` равен текущему времени, пустая правка
// ничего не меняет, а сам триггер не вызывает новых обновлений.

fn trigger_sql() -> &'static str {
    "CREATE FUNCTION touch_profile() RETURNS trigger LANGUAGE plpgsql AS $$
     BEGIN
         UPDATE profiles SET updated_at = now() WHERE user_id = NEW.user_id;
         RETURN NEW;
     END
     $$;
     CREATE TRIGGER profiles_touch AFTER UPDATE ON profiles FOR EACH ROW EXECUTE FUNCTION touch_profile()"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = "
        CREATE TABLE profiles (user_id bigint PRIMARY KEY, theme text NOT NULL, updated_at timestamptz NOT NULL DEFAULT now());
        INSERT INTO profiles VALUES (1, 'dark', '2020-01-01 00:00+00'), (2, 'light', '2020-01-01 00:00+00'), (3, 'auto', '2020-01-01 00:00+00');
    ";

    async fn setup() -> TestDb {
        let db = TestDb::with_sql(SEED).await;
        db.run(trigger_sql()).await;
        db
    }

    async fn is_fresh(db: &TestDb, id: i64) -> bool {
        db.table(&format!("SELECT updated_at > now() - interval '1 minute' FROM profiles WHERE user_id = {id}")).await.cell(0, 0) == "true"
    }

    #[tokio::test]
    async fn update_bumps_only_touched_rows() {
        let db = setup().await;
        let failed = db.fails("UPDATE profiles SET theme = 'light' WHERE user_id = 1").await;
        assert!(failed.is_none(), "обычная правка профиля падает: {failed:?}. Триггер, который сам обновляет таблицу, запускает себя бесконечно");
        assert!(is_fresh(&db, 1).await, "после правки updated_at должен стать текущим временем");
        assert!(!is_fresh(&db, 2).await, "соседний профиль не менялся, его updated_at трогать нельзя");
    }

    #[tokio::test]
    async fn one_statement_many_rows() {
        let db = setup().await;
        db.run("UPDATE profiles SET theme = theme || '!' WHERE user_id IN (2, 3)").await;
        assert!(is_fresh(&db, 2).await && is_fresh(&db, 3).await, "обе строки одной команды должны получить новое время");
        assert!(!is_fresh(&db, 1).await, "строка вне условия осталась прежней");
    }

    #[tokio::test]
    async fn empty_update_changes_nothing() {
        let db = setup().await;
        db.run("UPDATE profiles SET theme = 'dark' WHERE user_id = 1").await;
        assert!(!is_fresh(&db, 1).await, "значение темы то же самое, updated_at менять не нужно");
        let t = db.table("SELECT theme FROM profiles WHERE user_id = 1").await;
        assert_eq!(t.cell(0, 0), "dark", "данные на месте");
    }
}
