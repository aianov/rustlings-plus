fn starts_with_b() -> &'static str {
    "SELECT username FROM users WHERE username LIKE 'b%'"
}

fn mail_org() -> &'static str {
    "SELECT username FROM users WHERE email LIKE '%@mail.org'"
}

fn contains_ar() -> &'static str {
    "SELECT username FROM users WHERE username ILIKE '%ar%'"
}

fn four_letters() -> &'static str {
    "SELECT username FROM users WHERE username LIKE '____'"
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
