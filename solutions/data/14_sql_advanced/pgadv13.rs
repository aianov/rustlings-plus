fn schema_sql() -> &'static str {
    "CREATE TABLE notifications (
         id bigint GENERATED ALWAYS AS IDENTITY,
         user_id bigint NOT NULL,
         kind text NOT NULL,
         created_at timestamptz NOT NULL,
         PRIMARY KEY (id, created_at)
     ) PARTITION BY RANGE (created_at);
     CREATE TABLE notifications_2031_01 PARTITION OF notifications FOR VALUES FROM ('2031-01-01') TO ('2031-02-01');
     CREATE TABLE notifications_2031_02 PARTITION OF notifications FOR VALUES FROM ('2031-02-01') TO ('2031-03-01');
     CREATE TABLE notifications_2031_03 PARTITION OF notifications FOR VALUES FROM ('2031-03-01') TO ('2031-04-01')"
}

fn add_month(year: i32, month: u32) -> String {
    let (next_year, next_month) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
    format!(
        "CREATE TABLE notifications_{year}_{month:02} PARTITION OF notifications FOR VALUES FROM ('{year}-{month:02}-01') TO ('{next_year}-{next_month:02}-01')"
    )
}

fn retire_month(year: i32, month: u32) -> String {
    format!("ALTER TABLE notifications DETACH PARTITION notifications_{year}_{month:02}; DROP TABLE notifications_{year}_{month:02}")
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;


    async fn plan_text(db: &TestDb, sql: &str) -> String {
        db.table(&format!("EXPLAIN {sql}")).await.column(0).join("\n")
    }

    async fn part_of(db: &TestDb, id_filter: &str) -> String {
        db.table(&format!("SELECT tableoid::regclass::text FROM notifications WHERE {id_filter}")).await.cell(0, 0).to_string()
    }

    async fn seeded() -> TestDb {
        let db = TestDb::new().await;
        db.run(schema_sql()).await;
        db.run("INSERT INTO notifications (user_id, kind, created_at) VALUES
            (1, 'like', '2031-01-15 10:00+00'), (2, 'follow', '2031-01-31 23:59:59+00'),
            (1, 'like', '2031-02-01 00:00:00+00'), (3, 'comment', '2031-02-20 12:00+00'),
            (2, 'like', '2031-03-31 23:59:59+00')").await;
        db
    }

    #[tokio::test]
    async fn rows_land_in_their_month() {
        let db = seeded().await;
        let t = db.table("SELECT column_name FROM information_schema.columns WHERE table_schema = current_schema() AND table_name = 'notifications' ORDER BY ordinal_position").await;
        assert_eq!(t.column(0), vec!["id", "user_id", "kind", "created_at"], "столбцы родителя: id, user_id, kind, created_at");
        assert!(part_of(&db, "created_at = '2031-01-31 23:59:59+00'").await.ends_with("notifications_2031_01"), "последняя секунда января принадлежит январской партиции");
        assert!(part_of(&db, "created_at = '2031-02-01 00:00:00+00'").await.ends_with("notifications_2031_02"), "первая секунда февраля принадлежит февральской партиции (верхняя граница не включается)");
        assert!(part_of(&db, "created_at = '2031-03-31 23:59:59+00'").await.ends_with("notifications_2031_03"), "март");
        let err = db.fails("INSERT INTO notifications (user_id, kind, created_at) VALUES (1, 'like', '2031-04-01 00:00+00')").await;
        assert_eq!(err.map(|e| e.code), Some("23514".to_string()), "для апреля партиции нет, база должна отказать, а не молча сложить строку куда-то");
    }

    #[tokio::test]
    async fn queries_by_month_touch_one_partition() {
        let db = seeded().await;
        let plan = plan_text(&db, "SELECT * FROM notifications WHERE created_at >= '2031-02-01' AND created_at < '2031-03-01'").await;
        assert!(plan.contains("notifications_2031_02"), "февральские данные лежат в notifications_2031_02:\n{plan}");
        assert!(!plan.contains("notifications_2031_01") && !plan.contains("notifications_2031_03"), "соседние месяцы читаться не должны (отсечение партиций):\n{plan}");
    }

    #[tokio::test]
    async fn new_months_including_the_year_end() {
        let db = seeded().await;
        db.run(&add_month(2031, 4)).await;
        db.run("INSERT INTO notifications (user_id, kind, created_at) VALUES (1, 'like', '2031-04-30 23:59:59+00')").await;
        assert!(part_of(&db, "created_at = '2031-04-30 23:59:59+00'").await.ends_with("notifications_2031_04"), "апрельская партиция принимает последнюю секунду апреля");
        let err = db.fails("INSERT INTO notifications (user_id, kind, created_at) VALUES (1, 'like', '2031-05-01 00:00+00')").await;
        assert_eq!(err.map(|e| e.code), Some("23514".to_string()), "май ещё не создан, партиция апреля заканчивается ровно на 1 мая");
        db.run(&add_month(2031, 12)).await;
        db.run(&add_month(2032, 1)).await;
        db.run("INSERT INTO notifications (user_id, kind, created_at) VALUES (1, 'like', '2031-12-31 23:59:59+00'), (1, 'like', '2032-01-01 00:00:00+00')").await;
        assert!(part_of(&db, "created_at = '2031-12-31 23:59:59+00'").await.ends_with("notifications_2031_12"), "декабрь 2031");
        assert!(part_of(&db, "created_at = '2032-01-01 00:00:00+00'").await.ends_with("notifications_2032_01"), "январь 2032: декабрьская партиция заканчивается 1 января следующего года");
    }

    #[tokio::test]
    async fn retiring_a_month_drops_the_whole_table() {
        let db = seeded().await;
        db.run(&retire_month(2031, 1)).await;
        let gone = db.table("SELECT to_regclass('notifications_2031_01') IS NULL").await;
        assert_eq!(gone.cell(0, 0), "true", "партиция января должна быть удалена целиком (DROP), а не очищена DELETE: таблица осталась");
        assert_eq!(db.table("SELECT count(*) FROM notifications").await.cell(0, 0), "3", "остались февраль (2 строки) и март (1 строка)");
        let err = db.fails("INSERT INTO notifications (user_id, kind, created_at) VALUES (1, 'like', '2031-01-10 10:00+00')").await;
        assert_eq!(err.map(|e| e.code), Some("23514".to_string()), "январь снят, партиции для него больше нет");
    }
}
