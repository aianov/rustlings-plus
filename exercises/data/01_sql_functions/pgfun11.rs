// Интервалы пишутся как interval '1 day', interval '15 minutes'. Их прибавляют к времени и
// вычитают, а разность двух моментов времени тоже интервал.

fn last_day() -> &'static str {
    // TODO: id постов, написанных за последние 24 часа (от текущего момента), по возрастанию id
    "SELECT id FROM posts ORDER BY id"
}

fn account_age_days() -> &'static str {
    // TODO: username и сколько полных дней прошло с регистрации до момента
    // timestamptz '2026-10-01 00:00:00+00', под именем days, типа integer
    "SELECT username, 0 AS days FROM users"
}

fn edit_deadline() -> &'static str {
    // TODO: правка поста разрешена 15 минут. Верни id и время окончания правки под именем deadline
    // для поста с id = 1
    "SELECT id, created_at AS deadline FROM posts WHERE id = 1"
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
    async fn recent_posts() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(last_day()).await;
        assert_eq!(t.col("id"), ["13", "14", "15"], "за последние сутки написаны посты 13, 14 и 15:\n{t}");
    }

    #[tokio::test]
    async fn age_in_days() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(account_age_days()).await;
        assert_eq!(row(&t, "alice")[1], "994", "дней между регистрацией alice и 1 октября 2026:\n{t}");
        assert_eq!(row(&t, "grace")[1], "811");
    }

    #[tokio::test]
    async fn plus_interval() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(edit_deadline()).await;
        assert_eq!(t.cell(0, 1), "2026-03-01 10:15:00+00", "пост создан в 10:00, правка до 10:15:\n{t}");
    }
}
