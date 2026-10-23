// Хранилище открывается по имени типа из конфигурации: `mem` даёт хранилище в памяти, всё остальное
// заглушку, которая ничего не запоминает. Фабрика обещает вернуть «что-то, реализующее Storage», и это
// честно в каждой ветке по отдельности, но компилятор видит два разных типа там, где обещан один.
// TODO: Почини `open`, не меняя поведения хранилищ.

use std::collections::HashMap;

pub trait Storage {
    fn put(&mut self, key: &str, value: &str);
    fn get(&self, key: &str) -> Option<String>;
}

#[derive(Default)]
pub struct MemStorage {
    data: HashMap<String, String>,
}

pub struct NullStorage;

impl Storage for MemStorage {
    fn put(&mut self, key: &str, value: &str) {
        self.data.insert(key.to_string(), value.to_string());
    }

    fn get(&self, key: &str) -> Option<String> {
        self.data.get(key).cloned()
    }
}

impl Storage for NullStorage {
    fn put(&mut self, _key: &str, _value: &str) {}

    fn get(&self, _key: &str) -> Option<String> {
        None
    }
}

pub fn open(kind: &str) -> impl Storage {
    match kind {
        "mem" => MemStorage::default(),
        _ => NullStorage,
    }
}

fn main() {
    let mut s = open("mem");
    s.put("a", "1");
    println!("{:?}", s.get("a"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_storage_remembers() {
        let mut s = open("mem");
        s.put("k", "v");
        assert_eq!(s.get("k"), Some("v".to_string()));
        assert_eq!(s.get("other"), None);
    }

    #[test]
    fn null_storage_forgets() {
        let mut s = open("null");
        s.put("k", "v");
        assert_eq!(s.get("k"), None, "заглушка ничего не запоминает");
        let mut unknown = open("whatever");
        unknown.put("k", "v");
        assert_eq!(unknown.get("k"), None, "неизвестный тип тоже заглушка");
    }

    #[test]
    fn choice_can_be_made_at_run_time() {
        let kinds = ["mem", "null", "mem"];
        let mut stores: Vec<Box<dyn Storage>> = kinds.iter().map(|k| open(k)).collect();
        for s in stores.iter_mut() {
            s.put("a", "1");
        }
        let got: Vec<Option<String>> = stores.iter().map(|s| s.get("a")).collect();
        assert_eq!(got, vec![Some("1".to_string()), None, Some("1".to_string())]);
    }
}
