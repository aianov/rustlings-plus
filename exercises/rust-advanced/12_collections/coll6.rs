// Посты на странице автора идут по времени, а не как попало: ключом служит пара (время, id), и дерево само
// держит их по порядку. Нужно достать посты за промежуток, самый новый и самый старый, а ещё снять
// самый старый (устаревание). Выбирать из всего дерева фильтром можно, но дерево для того и ставили, чтобы
// не обходить его целиком.
// TODO: Допиши методы. `between(from, to)` берёт посты с временем от `from` включительно до `to`
// не включая, при одинаковом времени по возрастанию id.
use std::collections::BTreeMap;

#[derive(Default)]
pub struct Timeline {
    posts: BTreeMap<(u64, u64), String>,
}

impl Timeline {
    pub fn new() -> Timeline {
        Timeline::default()
    }

    pub fn add(&mut self, ts: u64, id: u64, text: &str) {
        self.posts.insert((ts, id), text.to_string());
    }

    pub fn len(&self) -> usize {
        self.posts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.posts.is_empty()
    }

    pub fn between(&self, from: u64, to: u64) -> Vec<(u64, u64)> {
        todo!("ключи (время, id) за промежуток")
    }

    pub fn newest(&self) -> Option<(u64, u64)> {
        todo!("самый новый ключ")
    }

    pub fn pop_oldest(&mut self) -> Option<String> {
        todo!("убрать самый старый пост и вернуть его текст")
    }
}

fn main() {
    let mut t = Timeline::new();
    t.add(10, 1, "hi");
    println!("{:?}", t.newest());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timeline() -> Timeline {
        let mut t = Timeline::new();
        t.add(30, 3, "c");
        t.add(10, 1, "a");
        t.add(20, 9, "b2");
        t.add(20, 2, "b1");
        t.add(40, 4, "d");
        t
    }

    #[test]
    fn a_window_is_half_open_and_ordered() {
        let t = timeline();
        assert_eq!(t.between(10, 30), vec![(10, 1), (20, 2), (20, 9)], "30 не входит, 10 входит, одинаковое время по id");
        assert_eq!(t.between(20, 21), vec![(20, 2), (20, 9)]);
        assert_eq!(t.between(0, 100).len(), 5);
    }

    #[test]
    fn empty_and_backwards_windows_are_empty() {
        let t = timeline();
        assert!(t.between(21, 30).is_empty());
        assert!(t.between(30, 10).is_empty(), "перевёрнутый промежуток не должен паниковать");
        assert!(t.between(20, 20).is_empty());
    }

    #[test]
    fn newest_and_pop_oldest() {
        let mut t = timeline();
        assert_eq!(t.newest(), Some((40, 4)));
        assert_eq!(t.pop_oldest().as_deref(), Some("a"));
        assert_eq!(t.pop_oldest().as_deref(), Some("b1"));
        assert_eq!(t.len(), 3);
        let mut e = Timeline::new();
        assert_eq!(e.newest(), None);
        assert_eq!(e.pop_oldest(), None);
        assert!(e.is_empty());
    }
}
