// spawn возвращает JoinHandle<T>, а когда сама задача возвращает Result<V, E>, ожидание
// хэндла даёт Result<Result<V, E>, JoinError>: внешний слой это судьба задачи (паника, отмена),
// внутренний это ответ самой задачи. Сейчас код не собирается.
// TODO: Почини обе функции так, чтобы вызывающий получал либо пользователя, либо одну ошибку
// (и ошибку задачи, и сбой самой задачи).
use std::time::Duration;
use tokio::task::JoinError;

#[derive(Debug, PartialEq)]
enum LoadError {
    NotFound(u32),
    Task(String),
}

impl From<JoinError> for LoadError {
    fn from(error: JoinError) -> Self {
        LoadError::Task(error.to_string())
    }
}

async fn find_user(id: u32) -> Result<String, LoadError> {
    tokio::time::sleep(Duration::from_millis(1)).await;
    if id == 0 {
        Err(LoadError::NotFound(id))
    } else {
        Ok(format!("user{id}"))
    }
}

async fn load_in_task(id: u32) -> Result<String, LoadError> {
    let handle = tokio::spawn(find_user(id));
    handle.await
}

async fn load_many(ids: &[u32]) -> Result<Vec<String>, LoadError> {
    let handles: Vec<_> = ids.iter().map(|id| tokio::spawn(find_user(*id))).collect();
    let mut users = Vec::new();
    for handle in handles {
        users.push(handle.await);
    }
    Ok(users)
}

#[tokio::main]
async fn main() {
    println!("{:?}", load_in_task(7).await);
    println!("{:?}", load_many(&[1, 2, 0]).await);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn found() {
        assert_eq!(load_in_task(5).await, Ok("user5".to_string()), "ответ задачи должен дойти без обёрток");
    }

    #[tokio::test]
    async fn task_error_comes_through() {
        assert_eq!(
            load_in_task(0).await,
            Err(LoadError::NotFound(0)),
            "внутренняя ошибка задачи (NotFound) должна выйти наружу как есть"
        );
    }

    #[tokio::test]
    async fn many_users() {
        assert_eq!(
            load_many(&[1, 2, 3]).await,
            Ok(vec!["user1".to_string(), "user2".to_string(), "user3".to_string()]),
            "load_many возвращает пользователей по порядку ids"
        );
    }

    #[tokio::test]
    async fn many_stops_on_first_error() {
        assert_eq!(load_many(&[1, 0, 3]).await, Err(LoadError::NotFound(0)), "первая ошибка прерывает сбор");
    }
}
