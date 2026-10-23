// Хештеги ищутся по индексу «тег -> посты». Пользователи жалуются на призраков: пост удалён, а в
// списке тега он остаётся (ссылка ведёт в никуда), а когда пост правят и убирают тег, в старом теге он тоже
// торчит. Теги берутся из текста по словам, начинающимся с `#`, в нижнем регистре.
// TODO: Найди, чего не делают `remove` и `edit`, и почини их. Тег, у которого не осталось постов, из индекса
// пропадает.
use std::collections::{BTreeSet, HashMap, HashSet};

pub fn tags_of(text: &str) -> HashSet<String> {
    text.split_whitespace().filter_map(|w| w.strip_prefix('#')).filter(|t| !t.is_empty()).map(str::to_lowercase).collect()
}

#[derive(Default)]
pub struct Store {
    texts: HashMap<u64, String>,
    by_tag: HashMap<String, BTreeSet<u64>>,
}

impl Store {
    pub fn add(&mut self, id: u64, text: &str) {
        for tag in tags_of(text) {
            self.by_tag.entry(tag).or_default().insert(id);
        }
        self.texts.insert(id, text.to_string());
    }

    pub fn with_tag(&self, tag: &str) -> Vec<u64> {
        self.by_tag.get(tag).map(|s| s.iter().copied().collect()).unwrap_or_default()
    }

    pub fn tags_known(&self) -> usize {
        self.by_tag.len()
    }

    pub fn text(&self, id: u64) -> Option<&str> {
        self.texts.get(&id).map(String::as_str)
    }

    pub fn remove(&mut self, id: u64) -> bool {
        self.texts.remove(&id).is_some()
    }

    pub fn edit(&mut self, id: u64, new_text: &str) -> bool {
        if !self.texts.contains_key(&id) {
            return false;
        }
        for tag in tags_of(new_text) {
            self.by_tag.entry(tag).or_default().insert(id);
        }
        self.texts.insert(id, new_text.to_string());
        true
    }
}

fn main() {
    let mut s = Store::default();
    s.add(1, "hello #rust");
    println!("{:?}", s.with_tag("rust"));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Store {
        let mut s = Store::default();
        s.add(1, "learning #Rust today");
        s.add(2, "#rust and #cats");
        s.add(3, "no tags here");
        s
    }

    #[test]
    fn tags_are_indexed_case_insensitively() {
        let s = store();
        assert_eq!(s.with_tag("rust"), vec![1, 2]);
        assert_eq!(s.with_tag("cats"), vec![2]);
        assert!(s.with_tag("dogs").is_empty());
    }

    #[test]
    fn a_removed_post_leaves_every_tag() {
        let mut s = store();
        assert!(s.remove(2));
        assert_eq!(s.with_tag("rust"), vec![1], "удалённый пост остался в индексе тега");
        assert!(s.with_tag("cats").is_empty());
        assert_eq!(s.tags_known(), 1, "тег cats остался без постов и должен пропасть из индекса");
        assert!(!s.remove(2));
        assert_eq!(s.text(2), None);
    }

    #[test]
    fn an_edit_moves_the_post_between_tags() {
        let mut s = store();
        assert!(s.edit(2, "now only #dogs"));
        assert_eq!(s.with_tag("rust"), vec![1], "старый тег убрали из текста, значит и из индекса");
        assert!(s.with_tag("cats").is_empty());
        assert_eq!(s.with_tag("dogs"), vec![2]);
        assert_eq!(s.text(2), Some("now only #dogs"));
        assert_eq!(s.tags_known(), 2);
    }

    #[test]
    fn editing_keeps_shared_tags_and_missing_posts_are_refused() {
        let mut s = store();
        assert!(s.edit(1, "still #rust"));
        assert_eq!(s.with_tag("rust"), vec![1, 2]);
        assert!(!s.edit(99, "#ghost"));
        assert!(s.with_tag("ghost").is_empty(), "правка несуществующего поста не должна создавать записей");
    }
}
