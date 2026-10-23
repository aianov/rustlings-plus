// Админка ищет бета-тестеров: `WHERE settings @> '{"beta": true}'`. В таблице сорок тысяч профилей,
// и база читает все, чтобы найти восемьдесят. Обычный B-tree по jsonb не поможет: он умеет только
// равенство целых документов. Нужен GIN. Из двух его вариантов для jsonb тебе подходит тот, что
// умеет только оператор содержания `@>` и взамен получается заметно компактнее: тест сравнит размер
// с индексом по умолчанию.
// TODO: Верни команду, которая создаёт нужный индекс на `profiles`.

fn index_sql() -> &'static str {
    "SELECT 1"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;


    async fn plan_text(db: &TestDb, sql: &str) -> String {
        db.table(&format!("EXPLAIN {sql}")).await.column(0).join("\n")
    }

    const SEED: &str = "
        CREATE TABLE profiles AS
            SELECT g AS user_id,
                   jsonb_build_object('theme', (ARRAY['dark','light','auto'])[1 + g % 3],
                                      'lang', (ARRAY['ru','en','de','fr'])[1 + g % 4],
                                      'beta', g % 500 = 0,
                                      'device', 'phone-' || (g % 997)) AS settings
            FROM generate_series(1, 40000) g;
        ANALYZE profiles;
    ";
    const QUERY: &str = r#"SELECT user_id FROM profiles WHERE settings @> '{"beta": true}'"#;

    #[tokio::test]
    async fn without_an_index_it_reads_everything() {
        let db = TestDb::with_sql(SEED).await;
        let plan = plan_text(&db, QUERY).await;
        assert!(plan.contains("Seq Scan on profiles"), "до индекса ожидался полный перебор, а план такой:\n{plan}");
    }

    #[tokio::test]
    async fn the_index_serves_containment() {
        let db = TestDb::with_sql(SEED).await;
        db.run(index_sql()).await;
        db.run("ANALYZE profiles").await;
        let plan = plan_text(&db, QUERY).await;
        assert!(plan.contains("Bitmap Index Scan") && !plan.contains("Seq Scan on profiles"), "запрос с @> должен идти по индексу, а план такой:\n{plan}");
        let t = db.table(QUERY).await;
        assert_eq!(t.len(), 80, "бета-тестеров должно быть 80 (каждый пятисотый из 40000)");
    }

    #[tokio::test]
    async fn the_index_is_the_compact_kind() {
        let db = TestDb::with_sql(SEED).await;
        db.run(index_sql()).await;
        let mine = db.table("SELECT indexname FROM pg_indexes WHERE schemaname = current_schema() AND tablename = 'profiles'").await;
        assert_eq!(mine.len(), 1, "после команды на таблице должен быть ровно один индекс:\n{mine}");
        let name = mine.cell(0, 0).to_string();
        db.run("CREATE INDEX reference_default_gin ON profiles USING gin (settings)").await;
        let sizes = db.table(&format!("SELECT pg_relation_size('{name}'), pg_relation_size('reference_default_gin')")).await;
        let (mine, reference): (f64, f64) = (sizes.cell(0, 0).parse().unwrap(), sizes.cell(0, 1).parse().unwrap());
        assert!(mine < reference * 0.8, "индекс должен быть заметно меньше стандартного GIN по jsonb ({mine} против {reference} байт): возьми класс операторов, который хранит хеши путей");
    }
}
