// Две функции вызывают async fn и забывают .await. Первая не собирается, а вторая
// собирается и молча ничего не делает: компилятор лишь предупреждает (unused_must_use),
// а предупреждения в длинном выводе легко пропустить.
// TODO: Найди оба места и заставь функции работать. Читай предупреждения компилятора.
use std::sync::{Arc, Mutex};
use std::time::Duration;

type Store = Arc<Mutex<Vec<String>>>;

async fn save_user(store: &Store, name: &str) {
    tokio::time::sleep(Duration::from_millis(1)).await;
    store.lock().unwrap().push(name.to_string());
}

async fn user_count(store: &Store) -> usize {
    tokio::time::sleep(Duration::from_millis(1)).await;
    store.lock().unwrap().len()
}

async fn register_all(store: &Store, names: &[&str]) -> usize {
    for name in names {
        save_user(store, name);
    }
    user_count(store)
}

#[tokio::main]
async fn main() {
    let store = Store::default();
    let saved = register_all(&store, &["аня", "борис"]).await;
    println!("зарегистрировано: {saved}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn every_name_lands_in_the_store() {
        let store = Store::default();
        let saved = register_all(&store, &["аня", "борис", "вера"]).await;
        assert_eq!(
            *store.lock().unwrap(),
            vec!["аня", "борис", "вера"],
            "хранилище пустое или неполное: save_user вызвана, но Future никто не дождался. Нужен .await"
        );
        assert_eq!(saved, 3, "register_all должна вернуть число пользователей в хранилище");
    }

    #[tokio::test]
    async fn empty_list_gives_zero() {
        let store = Store::default();
        assert_eq!(register_all(&store, &[]).await, 0, "для пустого списка ответ 0");
    }
}
