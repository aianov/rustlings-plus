use std::collections::HashMap;

pub struct Cache<F: Fn(&str) -> String> {
    map: HashMap<String, String>,
    loader: F,
    loads: u32,
}

impl<F: Fn(&str) -> String> Cache<F> {
    pub fn new(loader: F) -> Cache<F> {
        Cache { map: HashMap::new(), loader, loads: 0 }
    }

    pub fn loads(&self) -> u32 {
        self.loads
    }

    pub fn get(&mut self, key: &str) -> &String {
        if !self.map.contains_key(key) {
            self.loads += 1;
            let value = (self.loader)(key);
            self.map.insert(key.to_string(), value);
        }
        &self.map[key]
    }
}

fn main() {
    let mut cache = Cache::new(|k| k.to_uppercase());
    println!("{}", cache.get("rust"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_once_per_key() {
        let mut cache = Cache::new(|k| format!("definition of {k}"));
        assert_eq!(cache.get("lifetime"), "definition of lifetime");
        assert_eq!(cache.get("lifetime"), "definition of lifetime");
        assert_eq!(cache.loads(), 1, "второй запрос того же ключа не должен звать загрузчик");
        assert_eq!(cache.get("borrow"), "definition of borrow");
        assert_eq!(cache.loads(), 2);
    }

    #[test]
    fn returned_reference_is_usable() {
        let mut cache = Cache::new(|k| k.repeat(2));
        let len = cache.get("ab").len();
        assert_eq!(len, 4);
        let first = cache.get("ab").clone();
        let second = cache.get("cd").clone();
        assert_eq!((first.as_str(), second.as_str()), ("abab", "cdcd"));
    }

    #[test]
    fn empty_values_are_cached_too() {
        let mut cache = Cache::new(|_| String::new());
        cache.get("x");
        cache.get("x");
        assert_eq!(cache.loads(), 1, "пустая строка тоже значение, повторно не грузим");
    }
}
