// Пользователи хранятся по id, а вход и упоминания ищут по нику. Запрос по нику в таблице users просит
// ALLOW FILTERING. Нужна таблица поиска users_by_name (username, user_id), которая ведётся рядом. Ник
// при этом нечувствителен к регистру: Kate и kate это одно имя, но в профиле показывается так, как его
// написали при регистрации.
// TODO: Схема двух таблиц (users: user_id, username, ключ user_id), `register` пишет обе, `find` ищет id по нику.

fn schema() -> &'static str {
    "CREATE TABLE users (user_id int PRIMARY KEY, username text)"
}

fn register(user_id: i32, username: &str) -> String {
    format!("INSERT INTO users (user_id, username) VALUES ({user_id}, '{username}')")
}

fn find(username: &str) -> String {
    format!("SELECT user_id FROM users WHERE username = '{username}'")
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    async fn ready() -> TestKs {
        let ks = TestKs::with_cql(schema()).await;
        ks.run(&register(7, "Kate")).await;
        ks.run(&register(9, "bob")).await;
        ks
    }

    #[tokio::test]
    async fn find_ignores_case() {
        let ks = ready().await;
        for (name, id) in [("Kate", "7"), ("KATE", "7"), ("kate", "7"), ("Bob", "9")] {
            let t = ks.table(&find(name)).await;
            assert_eq!(t.column(0), [id], "ник {name} должен найти пользователя {id}\n{t}");
        }
        let none = ks.table(&find("nobody")).await;
        assert!(none.is_empty(), "неизвестный ник ничего не находит\n{none}");
    }

    #[tokio::test]
    async fn profile_keeps_original_spelling() {
        let ks = ready().await;
        let t = ks.table("SELECT username FROM users WHERE user_id = 7").await;
        assert_eq!(t.column(0), ["Kate"], "в профиле ник как при регистрации\n{t}");
        assert!(!find("Kate").to_uppercase().contains("ALLOW FILTERING"), "поиск по таблице поиска, без ALLOW FILTERING");
    }
}
