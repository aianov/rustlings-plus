// Чату нужно своё пространство ключей (keyspace): в нём будут жить все его таблицы. У keyspace
// обязательно задаётся репликация: сколько копий каждой строки держать. Узел у нас один, поэтому и копия
// одна. Скрипт развёртывания запускают при каждом старте сервиса, так что повторный запуск не должен падать.
// TODO: Верни команду, которая создаёт keyspace с именем `name` по этим правилам.

fn create_keyspace(name: &str) -> String {
    format!("CREATE KEYSPACE {name}")
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    async fn fresh() -> (TestKs, String) {
        let mut ks = TestKs::new().await;
        let name = format!("{}_chat", ks.name());
        ks.track_keyspace(&name);
        (ks, name)
    }

    #[tokio::test]
    async fn keyspace_has_one_replica() {
        let (ks, name) = fresh().await;
        if let Some(e) = ks.fails(&create_keyspace(&name)).await {
            panic!("Scylla отвергла команду: {e}");
        }
        let t = ks.table(&format!("SELECT replication FROM system_schema.keyspaces WHERE keyspace_name = '{name}'")).await;
        assert_eq!(t.len(), 1, "keyspace {name} так и не появился");
        let replication = t.cell(0, 0);
        assert!(replication.contains("SimpleStrategy"), "на одном узле достаточно SimpleStrategy, а у тебя: {replication}");
        assert!(replication.contains("replication_factor: 1"), "узел один, значит и копия одна: {replication}");
    }

    #[tokio::test]
    async fn second_run_does_not_fail() {
        let (ks, name) = fresh().await;
        assert!(ks.fails(&create_keyspace(&name)).await.is_none(), "первый запуск упал");
        if let Some(e) = ks.fails(&create_keyspace(&name)).await {
            panic!("повторный запуск при старте сервиса упал: {e}");
        }
    }
}
