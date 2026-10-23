// `collect` собирает итератор в коллекцию по ожидаемому типу: `Vec`, `HashMap`, `BTreeSet`, `String`.
// Но не всякий сбор очевиден: в `HashMap` из пар повторяющийся ключ молча затирает прежнее значение, а
// «сгруппировать» через `collect` нельзя вовсе. Пять функций над лентой постов.
// TODO: Реализуй функции. `likes_by_author` суммирует лайки по автору, `group_by_length` складывает слова
// одной длины в один список (в порядке появления).
use std::collections::{BTreeMap, BTreeSet, HashMap};

pub struct Post {
    pub id: u32,
    pub author: &'static str,
    pub likes: u32,
}

pub fn index_by_id(posts: &[Post]) -> HashMap<u32, &Post> {
    todo!()
}

pub fn distinct_authors(posts: &[Post]) -> BTreeSet<&'static str> {
    todo!()
}

pub fn initials(names: &[&str]) -> String {
    todo!("первые буквы в верхнем регистре, пустые имена пропускаются")
}

pub fn likes_by_author(posts: &[Post]) -> BTreeMap<&'static str, u32> {
    todo!()
}

pub fn group_by_length<'a>(words: &[&'a str]) -> BTreeMap<usize, Vec<&'a str>> {
    todo!()
}

fn main() {
    println!("{}", initials(&["ann", "bob"]));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed() -> Vec<Post> {
        vec![
            Post { id: 3, author: "bob", likes: 4 },
            Post { id: 1, author: "ann", likes: 10 },
            Post { id: 2, author: "bob", likes: 6 },
        ]
    }

    #[test]
    fn index_and_authors() {
        let f = feed();
        let by_id = index_by_id(&f);
        assert_eq!(by_id.len(), 3);
        assert_eq!(by_id[&2].author, "bob");
        let authors: Vec<_> = distinct_authors(&f).into_iter().collect();
        assert_eq!(authors, vec!["ann", "bob"], "без повторов и по алфавиту");
    }

    #[test]
    fn initials_collect_into_a_string() {
        assert_eq!(initials(&["ann", "bob", "", "élise"]), "ABÉ");
        assert_eq!(initials(&[]), "");
    }

    #[test]
    fn likes_are_summed_per_author() {
        let totals = likes_by_author(&feed());
        assert_eq!(totals.get("bob"), Some(&10), "лайки двух постов bob складываются, а не затирают друг друга");
        assert_eq!(totals.get("ann"), Some(&10));
        assert_eq!(totals.len(), 2);
    }

    #[test]
    fn words_are_grouped_by_length() {
        let groups = group_by_length(&["to", "be", "or", "not", "to", "be"]);
        assert_eq!(groups[&2], vec!["to", "be", "or", "to", "be"], "слова одной длины идут вместе, в порядке появления");
        assert_eq!(groups[&3], vec!["not"]);
        assert_eq!(groups.len(), 2);
    }
}
