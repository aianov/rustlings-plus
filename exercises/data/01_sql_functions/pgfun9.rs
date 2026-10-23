// COALESCE(a, b, c) отдаёт первое значение, которое не NULL. NULLIF(a, b) наоборот
// превращает a в NULL, если оно равно b. Вместе они чинят деление на ноль: делитель, равный 0,
// становится NULL, результат деления тоже NULL, а COALESCE подставляет запасное значение.

fn shown_name() -> &'static str {
    // TODO: username и «показываемое имя» под именем shown: display_name, а если его нет, то username
    "SELECT username, username AS shown FROM users"
}

fn safe_rate() -> &'static str {
    // TODO: id и доля лайков от просмотров под именем rate, три знака после запятой.
    // У постов с нулём просмотров должен получиться 0.000, а не ошибка деления на ноль.
    "SELECT id, 0 AS rate FROM posts"
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
    async fn fallback_name() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(shown_name()).await;
        assert_eq!(row(&t, "bob")[1], "bob", "у bob нет display_name, показываем ник:\n{t}");
        assert_eq!(row(&t, "kenji")[1], "Kenji", "у kenji есть display_name:\n{t}");
    }

    #[tokio::test]
    async fn division_by_zero_is_safe() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(safe_rate()).await;
        assert_eq!(row(&t, "3")[1], "0.000", "у поста 3 нет просмотров, доля 0.000:\n{t}");
        assert_eq!(row(&t, "2")[1], "0.250", "обычный пост считается как раньше:\n{t}");
    }
}
