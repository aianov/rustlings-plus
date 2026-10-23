// Строки режут и перекраивают несколько функций: LEFT(s, n) берёт первые n символов,
// SPLIT_PART(s, разделитель, номер) вырезает кусок (номера с единицы), REPLACE заменяет
// подстроку. Функции вкладываются друг в друга.

fn preview() -> &'static str {
    // TODO: id и первые 8 символов текста поста под именем preview
    "SELECT id, '' AS preview FROM posts"
}

fn mail_tld() -> &'static str {
    // TODO: username и «зона» почты (последняя часть домена: org, com, jp) под именем tld
    "SELECT username, '' AS tld FROM users"
}

fn slug() -> &'static str {
    // TODO: id и «слаг» поста под именем slug: текст маленькими буквами, пробелы заменены на дефисы
    "SELECT id, '' AS slug FROM posts"
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
    async fn left_part() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(preview()).await;
        assert_eq!(row(&t, "1")[1], "Hello wo", "первые 8 символов поста 1:\n{t}");
    }

    #[tokio::test]
    async fn nested_split() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(mail_tld()).await;
        assert_eq!(row(&t, "bob")[1], "org", "у bob@mail.org зона org:\n{t}");
        assert_eq!(row(&t, "kenji")[1], "jp", "у kenji@example.jp зона jp:\n{t}");
        assert_eq!(row(&t, "alice")[1], "com");
    }

    #[tokio::test]
    async fn replace_spaces() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(slug()).await;
        assert_eq!(row(&t, "2")[1], "rust-is-fun", "слаг поста 2:\n{t}");
        assert_eq!(row(&t, "3")[1], "coffee-first,-code-later", "запятая остаётся, меняются только пробелы:\n{t}");
    }
}
