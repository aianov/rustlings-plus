// Рейтинг авторов недели. Таблица weekly_scores (author_id, week, score) хранит очки за неделю;
// не у каждого автора есть очки в каждую неделю. Функция `leaderboard` получает $1, номер недели
// (integer). Нужны только авторы, у которых есть очки в эту неделю. Столбцы: username; place
// (место по очкам, равные очки делят место, после ничьей места пропускаются); behind (на сколько очков
// отстаёт от лидера недели); prev_place (место этого автора на прошлой неделе $1 - 1, NULL, если
// там его нет); moved (prev_place минус place: плюс означает подъём, NULL, если прошлого места нет).
// Места прошлой недели считаются по тем же правилам, среди авторов той недели. Порядок: place, username.
// TODO: Напиши запрос.

fn leaderboard() -> &'static str {
    "SELECT 1"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    async fn board(db: &TestDb, week: i32) -> Vec<Vec<String>> {
        let sql = format!("SELECT q.username::text, q.place::text, q.behind::text, COALESCE(q.prev_place::text, 'NULL'), COALESCE(q.moved::text, 'NULL') FROM ({}) AS q", leaderboard());
        let rows = sqlx::query_as::<_, (String, String, String, String, String)>(&sql)
            .bind(week)
            .fetch_all(&db.pool)
            .await
            .unwrap_or_else(|e| panic!("leaderboard({week}): {e}"));
        rows.into_iter().map(|r| vec![r.0, r.1, r.2, r.3, r.4]).collect()
    }

    fn line(cells: [&str; 5]) -> Vec<String> {
        cells.iter().map(|c| c.to_string()).collect()
    }

    #[tokio::test]
    async fn places_gaps_and_movement() {
        let db = TestDb::with_sql(SEED).await;
        assert_eq!(
            board(&db, 41).await,
            [
                line(["alice", "1", "0", "3", "2"]),
                line(["bob", "1", "0", "1", "0"]),
                line(["carol", "3", "20", "1", "-2"]),
                line(["dave", "3", "20", "4", "1"]),
                line(["eve", "5", "50", "NULL", "NULL"]),
            ],
            "неделя 41: alice и bob делят первое место, carol и dave третье (четвёртого нет), eve новичок без прошлого места; на прошлой неделе bob и carol делили первое, alice была третьей, dave четвёртым; frank на этой неделе не играл и в списке быть не должен"
        );
    }

    #[tokio::test]
    async fn first_week_has_no_history() {
        let db = TestDb::with_sql(SEED).await;
        assert_eq!(
            board(&db, 40).await,
            [
                line(["bob", "1", "0", "NULL", "NULL"]),
                line(["carol", "1", "0", "NULL", "NULL"]),
                line(["alice", "3", "20", "NULL", "NULL"]),
                line(["dave", "4", "30", "NULL", "NULL"]),
                line(["frank", "5", "50", "NULL", "NULL"]),
            ],
            "неделя 40: прошлой недели нет, поэтому prev_place и moved везде NULL"
        );
    }

    #[tokio::test]
    async fn previous_places_come_from_the_data() {
        let db = TestDb::with_sql(SEED).await;
        db.run("INSERT INTO weekly_scores VALUES (5, 40, 70)").await;
        assert_eq!(
            board(&db, 41).await,
            [
                line(["alice", "1", "0", "4", "3"]),
                line(["bob", "1", "0", "1", "0"]),
                line(["carol", "3", "20", "1", "-2"]),
                line(["dave", "3", "20", "5", "2"]),
                line(["eve", "5", "50", "1", "-4"]),
            ],
            "после того как eve получила 70 очков на прошлой неделе, на первом месте их стало трое, а alice сползла на четвёртое: прошлые места считаются из таблицы, а не заранее"
        );
    }
}
