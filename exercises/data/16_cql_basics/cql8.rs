// Пользователь в настройках меняет только био. Клиент присылает новое био, сервис пишет его командой
// ниже, и после сохранения у kate пропадает аватар. Никакой ошибки не было, запись «прошла успешно».
// Разберись, почему INSERT в Scylla так себя ведёт, и сделай так, чтобы менялось только био kate (user_id 7),
// а остальные столбцы оставались как были. Новое био: «пишу на Rust».

fn save_bio() -> &'static str {
    "INSERT INTO profiles (user_id, username, bio, avatar) VALUES (7, 'kate', 'пишу на Rust', null)"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    const SCHEMA: &str = "CREATE TABLE profiles (user_id int PRIMARY KEY, username text, bio text, avatar text)";
    const ROWS: &str = "INSERT INTO profiles (user_id, username, bio, avatar) VALUES (7, 'kate', 'старое био', 'kate.png'); INSERT INTO profiles (user_id, username, bio, avatar) VALUES (9, 'bob', 'привет', 'bob.png')";

    #[tokio::test]
    async fn only_bio_changes() {
        let ks = TestKs::with_cql(SCHEMA).await;
        ks.run(ROWS).await;
        ks.run(save_bio()).await;
        let t = ks.table("SELECT username, bio, avatar FROM profiles WHERE user_id = 7").await;
        assert_eq!(t.rows, [["kate", "пишу на Rust", "kate.png"]], "био новое, имя и аватар прежние\n{t}");
        let other = ks.table("SELECT bio, avatar FROM profiles WHERE user_id = 9").await;
        assert_eq!(other.rows, [["привет", "bob.png"]], "чужой профиль не трогаем\n{other}");
    }

    #[tokio::test]
    async fn saving_twice_is_harmless() {
        let ks = TestKs::with_cql(SCHEMA).await;
        ks.run(ROWS).await;
        ks.run(save_bio()).await;
        ks.run(save_bio()).await;
        let t = ks.table("SELECT username, bio, avatar FROM profiles WHERE user_id = 7").await;
        assert_eq!(t.rows, [["kate", "пишу на Rust", "kate.png"]], "повтор той же записи ничего не портит\n{t}");
    }
}
