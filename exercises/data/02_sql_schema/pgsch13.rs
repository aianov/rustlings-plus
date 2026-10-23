// Схема это пространство имён внутри базы: в каждой свои таблицы, и одинаковые имена не
// конфликтуют. Какую схему считать «текущей» для имён без префикса, решает search_path
// (на нём же держатся все тесты курса: у каждого теста своя схема).
// Тест передаёт тебе два имени схем. Напиши скрипт: создай обе схемы, в каждой таблицу notes
// (id integer, text text); добавь строку 'in a' в таблицу первой схемы, указав схему в имени;
// потом переключи search_path на вторую и добавь строку 'in b' в notes БЕЗ префикса.
// TODO: Напиши скрипт.

fn two_schemas(a: &str, b: &str) -> String {
    // TODO: верни скрипт, подставив имена схем a и b
    let _ = (a, b);
    String::new()
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
