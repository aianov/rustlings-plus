// Строки склеиваются оператором ||. LOWER и UPPER меняют регистр, TRIM убирает пробелы по
// краям, LENGTH считает символы. Склейка с NULL даёт NULL, поэтому для необязательных полей
// есть concat_ws: она склеивает через разделитель и пропускает пустые значения.

fn tag() -> &'static str {
    // TODO: username и тег «@ник» маленькими буквами под именем tag
    "SELECT username, '' AS tag FROM users"
}

fn clean_display() -> &'static str {
    // TODO: username и display_name без пробелов по краям под именем display
    "SELECT username, display_name AS display FROM users"
}

fn title_line() -> &'static str {
    // TODO: username и строка «ник пробел отображаемое имя» под именем line.
    // Пробелов по краям имени быть не должно, а если display_name пусто, то строка это просто ник
    "SELECT username, username AS line FROM users"
}

fn body_length() -> &'static str {
    // TODO: id и число символов в тексте поста под именем len
    "SELECT id, 0 AS len FROM posts"
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
    async fn lower_and_concat() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(tag()).await;
        assert_eq!(row(&t, "kenji")[1], "@kenji", "тег это «@» и ник:\n{t}");
    }

    #[tokio::test]
    async fn trim_spaces() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(clean_display()).await;
        assert_eq!(row(&t, "alice")[1], "Alice A.", "пробелы по краям нужно убрать:\n{t}");
        assert_eq!(row(&t, "bob")[1], "NULL", "у bob display_name пусто, так и должно остаться");
    }

    #[tokio::test]
    async fn concat_skips_null() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(title_line()).await;
        assert_eq!(row(&t, "carol")[1], "carol Carol", "ник, пробел, имя:\n{t}");
        assert_eq!(row(&t, "alice")[1], "alice Alice A.", "лишние пробелы вокруг имени не нужны:\n{t}");
        assert_eq!(row(&t, "bob")[1], "bob", "без имени остаётся один ник, а не NULL: склейка || с NULL даёт NULL:\n{t}");
    }

    #[tokio::test]
    async fn length_of_text() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(body_length()).await;
        assert_eq!(row(&t, "2")[1], "11", "«Rust is fun» это 11 символов:\n{t}");
    }
}
