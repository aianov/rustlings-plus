// Карточка профиля: поля, которых нет в таблице, считаются в запросе. Пять запросов по
// условию, всё из этой главы: деление дат, CASE, COALESCE/NULLIF, пояса. «Сегодня» для всех
// запросов зафиксировано как timestamptz '2026-10-01 00:00:00+00', чтобы тесты не зависели от часов.

fn account_age() -> &'static str {
    // 1. username и сколько полных дней прошло от регистрации до «сегодня», под именем days (integer).
    todo!()
}

fn activity() -> &'static str {
    // 2. username и метка активности под именем label по last_login_at:
    //   нет входов: 'never'; последний вход позже, чем 7 суток назад: 'active'; позже, чем 30 суток назад:
    //   'sleeping'; иначе 'gone'. Вход ровно 7 суток назад уже не active.
    todo!()
}

fn bio_line() -> &'static str {
    // 3. username и биография под именем bio; если биографии нет, она пустая или из одних
    // пробелов, показать 'no bio'.
    todo!()
}

fn display() -> &'static str {
    // 4. username и имя для показа под именем display: display_name без пробелов по краям,
    // а если его нет, то username.
    todo!()
}

fn joined_local() -> &'static str {
    // 5. username и дата регистрации в часовом поясе пользователя (столбец tz) под именем joined.
    todo!()
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::{Table, TestDb};

    const SEED: &str = include_str!("seed.sql");

    fn row<'a>(t: &'a Table, key: &str) -> &'a Vec<String> {
        t.rows
            .iter()
            .find(|r| r[0] == key)
            .unwrap_or_else(|| panic!("в результате нет строки с ключом {key}:\n{t}"))
    }

    #[tokio::test]
    async fn age_of_account() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(account_age()).await;
        assert_eq!(t.columns, ["username", "days"], "нужны столбцы username и days");
        assert_eq!(row(&t, "alice")[1], "994", "полных дней от регистрации alice до 1 октября 2026:\n{t}");
        assert_eq!(row(&t, "heidi")[1], "785");
    }

    #[tokio::test]
    async fn activity_label() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(activity()).await;
        assert_eq!(t.columns, ["username", "label"], "нужны столбцы username и label");
        for (who, want) in [
            ("alice", "active"),
            ("bob", "sleeping"),
            ("carol", "gone"),
            ("dave", "never"),
            ("kenji", "active"),
            ("eve", "gone"),
            ("frank", "gone"),
            ("grace", "sleeping"),
            ("heidi", "never"),
        ] {
            assert_eq!(row(&t, who)[1], want, "метка пользователя {who}: NULL проверяй первым, границы (ровно 7 и ровно 30 суток) уже в следующей категории:\n{t}");
        }
    }

    #[tokio::test]
    async fn bio_fallback() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(bio_line()).await;
        assert_eq!(t.columns, ["username", "bio"], "нужны столбцы username и bio");
        assert_eq!(row(&t, "alice")[1], "Rust and coffee");
        for who in ["bob", "carol", "dave"] {
            assert_eq!(row(&t, who)[1], "no bio", "у {who} биографии нет (NULL, пусто или пробелы):\n{t}");
        }
    }

    #[tokio::test]
    async fn display_name() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(display()).await;
        assert_eq!(t.columns, ["username", "display"], "нужны столбцы username и display");
        assert_eq!(row(&t, "alice")[1], "Alice A.", "пробелы по краям убираем");
        assert_eq!(row(&t, "bob")[1], "bob", "нет имени, показываем ник");
    }

    #[tokio::test]
    async fn local_date() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(joined_local()).await;
        assert_eq!(t.columns, ["username", "joined"], "нужны столбцы username и joined");
        assert_eq!(row(&t, "kenji")[1], "2024-03-02");
        assert_eq!(row(&t, "bob")[1], "2024-02-29");
    }
}
