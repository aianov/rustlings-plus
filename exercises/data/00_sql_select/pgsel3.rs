// В списке SELECT можно считать: арифметика, функции, константы. А чтобы у столбца было
// человеческое имя, ему дают псевдоним через AS.
// Собери карточку пользователя из трёх столбцов.

fn card() -> &'static str {
    // TODO: Верни три столбца:
    //   username под именем login,
    //   возраст в месяцах (age * 12) под именем age_in_months,
    //   возраст через год (age + 1) под именем next_age.
    "SELECT username, age, age FROM users"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn aliases_are_applied() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(card()).await;
        assert_eq!(t.columns, ["login", "age_in_months", "next_age"], "имена и порядок столбцов не те, что в условии");
    }

    #[tokio::test]
    async fn values_are_computed() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(card()).await;
        let alice = t.rows.iter().find(|r| r[0] == "alice").expect("в результате нет пользователя alice");
        assert_eq!(alice[1], "336", "alice 28 лет, это 336 месяцев: умножай age на 12");
        assert_eq!(alice[2], "29", "через год alice будет 29: прибавь к age единицу");
        assert_eq!(t.len(), 12, "пользователей должно быть 12");
    }
}
