// LIKE ищет по шаблону: % это любое количество символов (даже ноль), _ ровно один символ.
// LIKE различает регистр, ILIKE нет (это расширение Postgres).

fn starts_with_b() -> &'static str {
    // TODO: ники, которые начинаются с буквы b (маленькой)
    "SELECT username FROM users"
}

fn mail_org() -> &'static str {
    // TODO: почта оканчивается на @mail.org
    "SELECT username FROM users"
}

fn contains_ar() -> &'static str {
    // TODO: в нике есть «ar» в любом регистре, где угодно внутри
    "SELECT username FROM users"
}

fn four_letters() -> &'static str {
    // TODO: ник ровно из четырёх символов
    "SELECT username FROM users"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    fn set(mut v: Vec<&str>) -> Vec<&str> {
        v.sort();
        v
    }

    async fn run(sql: &str) -> Vec<String> {
        let db = TestDb::with_sql(SEED).await;
        db.table(sql).await.col("username").into_iter().map(String::from).collect()
    }

    fn check(got: Vec<String>, want: Vec<&str>, what: &str) {
        let got = set(got.iter().map(String::as_str).collect());
        assert_eq!(got, set(want), "{what}: ответ не совпал с ожидаемым");
    }

    #[tokio::test]
    async fn prefix() {
        check(run(starts_with_b()).await, vec!["bob", "bob_2"], "ники на b");
    }

    #[tokio::test]
    async fn suffix() {
        check(run(mail_org()).await, vec!["carol", "frank", "ivan", "bob_2"], "почта на @mail.org");
    }

    #[tokio::test]
    async fn case_insensitive_infix() {
        check(run(contains_ar()).await, vec!["carol", "Karl"], "«ar» в любом регистре (Karl пишется с заглавной)");
    }

    #[tokio::test]
    async fn single_char_wildcard() {
        check(run(four_letters()).await, vec!["dave", "ivan", "judy", "Karl"], "ровно четыре символа");
    }
}
