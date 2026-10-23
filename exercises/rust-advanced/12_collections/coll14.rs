// Ники в соцсети различаются без учёта регистра: «Ira», «IRA» и «ira» это один человек. Хочется класть
// их в `HashMap`/`HashSet` как есть, не приводя строку к нижнему регистру на каждом входе. Для этого
// у типа свои `==` и хеш. Они обязаны быть согласованы: равные значения дают равный хеш, иначе карта
// будет терять ключи. Регистр понимаем по-юникодному: «ÄRGER» и «ärger» тоже один ник.
// TODO: Реализуй `PartialEq`, `Eq` и `Hash` для `Handle`.
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone)]
pub struct Handle(pub String);

impl PartialEq for Handle {
    fn eq(&self, other: &Handle) -> bool {
        todo!("равенство без учёта регистра")
    }
}

impl Eq for Handle {}

impl Hash for Handle {
    fn hash<H: Hasher>(&self, state: &mut H) {
        todo!("хеш, согласованный с равенством")
    }
}

fn main() {
    println!("{}", Handle("Ira".into()) == Handle("IRA".into()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};
    use std::hash::DefaultHasher;

    fn h(s: &str) -> Handle {
        Handle(s.to_string())
    }

    fn hash_of(x: &Handle) -> u64 {
        let mut hasher = DefaultHasher::new();
        x.hash(&mut hasher);
        hasher.finish()
    }

    #[test]
    fn case_does_not_matter() {
        assert_eq!(h("Ira"), h("IRA"));
        assert_eq!(h("ira"), h("iRa"));
        assert_ne!(h("ira"), h("iran"));
        assert_ne!(h("ira"), h("ora"));
        assert_eq!(h("ÄRGER"), h("ärger"), "регистр не только латинский");
    }

    #[test]
    fn equal_handles_have_equal_hashes() {
        for (a, b) in [("Ira", "IRA"), ("ÄRGER", "ärger"), ("MiXeD", "mixed"), ("", "")] {
            assert_eq!(h(a), h(b));
            assert_eq!(hash_of(&h(a)), hash_of(&h(b)), "{a} и {b} равны, а хеши разные: карта их потеряет");
        }
    }

    #[test]
    fn a_set_treats_them_as_one_user() {
        let mut set = HashSet::new();
        assert!(set.insert(h("Ira")));
        assert!(!set.insert(h("IRA")), "тот же ник другим регистром вставлять второй раз нельзя");
        assert!(set.insert(h("Oleg")));
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn a_map_finds_by_any_spelling() {
        let mut followers: HashMap<Handle, u32> = (0..100).map(|i| (h(&format!("User{i}")), i)).collect();
        for i in 0..100 {
            assert_eq!(followers.get(&h(&format!("uSER{i}"))), Some(&i), "ник user{i} не найден");
        }
        *followers.entry(h("USER7")).or_insert(0) += 1000;
        assert_eq!(followers.len(), 100);
        assert_eq!(followers[&h("user7")], 1007);
    }
}
