// Лента постов должна красиво печататься: каждый пост с номером на отдельной строке. Автор написал
// `impl Display for Vec<Post>`, и компилятор ответил, что так нельзя: и трейт `Display`, и тип `Vec`
// определены не в нашем крейте. Это правило сироты: чтобы две библиотеки не могли навязать конфликтующие
// реализации, реализовать чужой трейт для чужого типа нельзя. Выход: свой тип-обёртка.
// TODO: Почини печать. Тесты ожидают, что ленту можно собрать из вектора через `Feed::from`, узнать длину
// как у вектора (`feed.len()`) и напечатать через `to_string()` в формате `1. title` по строке на пост.

use std::fmt;

pub struct Post {
    pub title: String,
}

impl fmt::Display for Vec<Post> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, post) in self.iter().enumerate() {
            writeln!(f, "{}. {}", i + 1, post.title)?;
        }
        Ok(())
    }
}

pub struct Feed(pub Vec<Post>);

fn main() {
    let feed = Feed::from(vec![Post { title: "hello".into() }]);
    print!("{feed}");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn posts(titles: &[&str]) -> Vec<Post> {
        titles.iter().map(|t| Post { title: t.to_string() }).collect()
    }

    #[test]
    fn prints_numbered_lines() {
        let feed = Feed::from(posts(&["first", "second"]));
        assert_eq!(feed.to_string(), "1. first\n2. second\n");
    }

    #[test]
    fn empty_feed_prints_nothing() {
        assert_eq!(Feed::from(vec![]).to_string(), "");
    }

    #[test]
    fn feed_behaves_like_a_vector_for_reading() {
        let feed = Feed::from(posts(&["a", "b", "c"]));
        assert_eq!(feed.len(), 3, "длина берётся у вектора внутри");
        assert_eq!(feed.iter().map(|p| p.title.as_str()).collect::<Vec<_>>(), vec!["a", "b", "c"]);
        assert_eq!(feed.first().map(|p| p.title.as_str()), Some("a"));
    }
}
