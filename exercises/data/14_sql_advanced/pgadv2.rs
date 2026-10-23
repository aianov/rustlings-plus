// Маркетинг просит две выгрузки по настройкам. Первая: у кого тёмная тема и включена почта
// (в `notifications.email` настоящий `true`, а не строка "true": такие записи остались от старого
// клиента и считаются выключенными). Вторая: у кого вообще задан флаг `beta`, неважно, включён он, выключен
// или записан как null: это участники программы, которых надо спросить отдельно.
// TODO: Допиши `dark_with_email` и `beta_known`. Оба возвращают один столбец `user_id` по возрастанию.

fn dark_with_email() -> &'static str {
    "SELECT 1"
}

fn beta_known() -> &'static str {
    "SELECT 1"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = r#"
        CREATE TABLE profiles (user_id bigint PRIMARY KEY, settings jsonb NOT NULL);
        INSERT INTO profiles VALUES
            (1, '{"theme":"dark","notifications":{"email":true,"push":true},"beta":true}'),
            (2, '{"theme":"dark","notifications":{"email":false}}'),
            (3, '{"theme":"light","notifications":{"email":true},"beta":false}'),
            (4, '{"theme":"dark","notifications":{"email":true},"beta":null}'),
            (5, '{"theme":"dark","notifications":{"email":"true"}}'),
            (6, '{"theme":"dark","lang":"ru","notifications":{"push":true,"email":true}}'),
            (7, '{"theme":"dark","notifications":{"push":true}}');
    "#;

    #[tokio::test]
    async fn containment_matches_type_and_nesting() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(dark_with_email()).await;
        assert_eq!(t.column(0), vec!["1", "4", "6"], "нужны тёмная тема и email = настоящий true; у пользователя 5 в email строка \"true\", её считать нельзя:\n{t}");
    }

    #[tokio::test]
    async fn key_existence_does_not_care_about_the_value() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(beta_known()).await;
        assert_eq!(t.column(0), vec!["1", "3", "4"], "флаг beta задан у 1 (true), 3 (false) и 4 (null); проверка «значение не NULL» теряет четвёртого:\n{t}");
    }

    #[tokio::test]
    async fn both_see_new_rows() {
        let db = TestDb::with_sql(SEED).await;
        db.run(r#"INSERT INTO profiles VALUES (8, '{"theme":"dark","notifications":{"email":true},"beta":0}')"#).await;
        assert_eq!(db.table(dark_with_email()).await.column(0), vec!["1", "4", "6", "8"], "новый пользователь 8 подходит под первую выгрузку");
        assert_eq!(db.table(beta_known()).await.column(0), vec!["1", "3", "4", "8"], "и под вторую");
    }
}
