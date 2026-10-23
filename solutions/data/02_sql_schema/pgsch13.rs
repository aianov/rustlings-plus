fn two_schemas(a: &str, b: &str) -> String {
    format!(
        "CREATE SCHEMA {a};
         CREATE SCHEMA {b};
         CREATE TABLE {a}.notes (id integer, text text);
         CREATE TABLE {b}.notes (id integer, text text);
         INSERT INTO {a}.notes VALUES (1, 'in a');
         SET search_path TO {b};
         INSERT INTO notes VALUES (1, 'in b');"
    )
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;


    #[tokio::test]
    async fn each_schema_has_its_own_notes() {
        let mut db = TestDb::new().await;
        let a = format!("{}_a", db.schema());
        let b = format!("{}_b", db.schema());
        db.track_schema(&a);
        db.track_schema(&b);
        db.run(&two_schemas(&a, &b)).await;
        let ta = db.table(&format!("SELECT text FROM {a}.notes")).await;
        let tb = db.table(&format!("SELECT text FROM {b}.notes")).await;
        assert_eq!(ta.column(0), ["in a"], "в первой схеме должна быть ровно одна строка 'in a':\n{ta}");
        assert_eq!(tb.column(0), ["in b"], "во второй схеме должна быть ровно одна строка 'in b' (добавлена через search_path):\n{tb}");
    }
}
