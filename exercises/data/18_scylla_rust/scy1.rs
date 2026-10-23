// Сервису нужна сессия Scylla. Адрес узла берётся из переменной окружения RUSTLINGS_SCYLLA_URI, а если её
// нет или она пустая, используется адрес по умолчанию. Подключение к недоступному узлу не должно висеть
// вечно: на установку соединения отводится 3 секунды.
// TODO: `node` выбирает адрес, `connect` строит сессию через SessionBuilder.

use scylla::transport::errors::NewSessionError;
use scylla::{Session, SessionBuilder};
use std::time::Duration;

const DEFAULT_NODE: &str = "127.0.0.1:59042";

fn node(env: Option<String>) -> String {
    let _ = env;
    todo!("адрес из переменной или по умолчанию")
}

async fn connect(node: &str) -> Result<Session, NewSessionError> {
    let _ = (node, Duration::from_secs(3), SessionBuilder::new());
    todo!("сессия через SessionBuilder")
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_from_env_or_default() {
        assert_eq!(node(None), DEFAULT_NODE, "без переменной берётся адрес по умолчанию");
        assert_eq!(node(Some("10.1.2.3:9042".to_string())), "10.1.2.3:9042", "переменная задаёт адрес");
        assert_eq!(node(Some("  ".to_string())), DEFAULT_NODE, "пустая переменная это не адрес");
    }

    #[tokio::test]
    async fn connects_to_course_scylla() {
        let session = connect(&node(std::env::var("RUSTLINGS_SCYLLA_URI").ok())).await.expect("подключение к Scylla курса");
        let res = session.query("SELECT release_version FROM system.local", ()).await.expect("запрос к system.local");
        assert_eq!(res.rows_num().expect("ответ со строками"), 1, "system.local это одна строка");
    }

    #[tokio::test]
    async fn dead_node_is_an_error_not_a_hang() {
        let res = tokio::time::timeout(Duration::from_secs(20), connect("127.0.0.1:1"))
            .await
            .expect("подключение к мёртвому узлу должно закончиться ошибкой, а не висеть");
        assert!(res.is_err(), "узла нет, сессия не должна построиться");
    }
}
