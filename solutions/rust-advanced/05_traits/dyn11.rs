use std::fmt;
use std::ops::Deref;

pub struct Post {
    pub title: String,
}

pub struct Feed(pub Vec<Post>);

impl From<Vec<Post>> for Feed {
    fn from(posts: Vec<Post>) -> Feed {
        Feed(posts)
    }
}

impl Deref for Feed {
    type Target = Vec<Post>;

    fn deref(&self) -> &Vec<Post> {
        &self.0
    }
}

impl fmt::Display for Feed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, post) in self.0.iter().enumerate() {
            writeln!(f, "{}. {}", i + 1, post.title)?;
        }
        Ok(())
    }
}

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
