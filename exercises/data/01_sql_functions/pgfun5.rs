// Тип значения можно поменять: CAST(x AS тип) или короче x::тип. Это нужно, когда число
// пришло строкой, когда у числа нужно спросить про его запись (сколько цифр) или когда из текста
// нужна дата. Три небольших запроса.

fn sum_of_strings() -> &'static str {
    // Два числа пришли строками: '17' и '25'. TODO: сложи их как числа, результат назови n
    "SELECT 0 AS n"
}

fn digits() -> &'static str {
    // TODO: id и сколько цифр в числе просмотров под именем digits
    "SELECT id, 0 AS digits FROM posts"
}

fn plus_month() -> &'static str {
    // TODO: к дате '2026-03-17' прибавить 30 дней, результат назвать d
    "SELECT 0 AS d"
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
    async fn cast_to_integer() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(sum_of_strings()).await;
        assert_eq!(t.cell(0, 0), "42", "'17' и '25' как числа дают 42, а не склейку '1725':\n{t}");
    }

    #[tokio::test]
    async fn digits_of_number() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(digits()).await;
        assert_eq!(row(&t, "7")[1], "4", "в 1800 четыре цифры:\n{t}");
        assert_eq!(row(&t, "3")[1], "1", "в 0 одна цифра:\n{t}");
        assert_eq!(row(&t, "4")[1], "3", "в 300 три цифры:\n{t}");
    }

    #[tokio::test]
    async fn cast_to_date() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(plus_month()).await;
        assert_eq!(t.cell(0, 0), "2026-04-16", "дата плюс 30 дней (результат должен быть датой):\n{t}");
    }
}
