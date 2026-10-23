// CASE WHEN это if/else внутри запроса: проверяет условия сверху вниз и берёт первое
// подходящее, а без совпадений отдаёт ELSE (или NULL, если ELSE нет).

fn heat() -> &'static str {
    // TODO: id и метка поста под именем heat:
    //   лайков 100 и больше: 'hot'; от 20 до 99: 'popular'; от 1 до 19: 'normal'; иначе 'quiet'
    "SELECT id, 'quiet' AS heat FROM posts"
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
    async fn labels() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(heat()).await;
        for (id, want) in [("7", "hot"), ("10", "hot"), ("9", "popular"), ("2", "popular"), ("19", "popular"), ("6", "normal"), ("12", "normal"), ("3", "normal"), ("5", "quiet")] {
            assert_eq!(row(&t, id)[1], want, "метка поста {id}: границы включаются слева (100 уже hot, 99 ещё popular, 20 уже popular, 1 уже normal):\n{t}");
        }
    }
}
