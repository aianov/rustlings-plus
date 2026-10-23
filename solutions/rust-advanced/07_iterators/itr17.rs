pub struct Post {
    pub author: String,
    pub text: String,
    pub likes: u32,
}

pub struct Feed {
    pub posts: Vec<Post>,
}

impl Feed {
    pub fn by_author(&self, author: &str) -> impl Iterator<Item = &Post> {
        self.posts.iter().filter(move |p| p.author == author)
    }

    pub fn long_texts(&self, min: usize) -> impl Iterator<Item = &str> {
        self.posts.iter().map(|p| p.text.as_str()).filter(move |t| t.chars().count() >= min)
    }

    pub fn top(&self, n: usize) -> impl Iterator<Item = &Post> {
        let mut ranked: Vec<&Post> = self.posts.iter().collect();
        ranked.sort_by_key(|p| std::cmp::Reverse(p.likes));
        ranked.into_iter().take(n)
    }
}

pub fn total_likes<'a>(posts: impl Iterator<Item = &'a Post>) -> u32 {
    posts.map(|p| p.likes).sum()
}

fn main() {
    let feed = Feed { posts: vec![] };
    println!("{}", total_likes(feed.by_author("ann")));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed() -> Feed {
        let p = |author: &str, text: &str, likes| Post { author: author.into(), text: text.into(), likes };
        Feed { posts: vec![p("ann", "hello world", 5), p("bob", "hi", 9), p("ann", "ok", 9), p("cat", "a long enough text", 1)] }
    }

    #[test]
    fn filtering_by_author_is_lazy_and_ordered() {
        let f = feed();
        let texts: Vec<&str> = f.by_author("ann").map(|p| p.text.as_str()).collect();
        assert_eq!(texts, vec!["hello world", "ok"]);
        assert_eq!(f.by_author("nobody").count(), 0);
        let mut it = f.by_author("ann");
        assert_eq!(it.next().map(|p| p.likes), Some(5), "можно брать по одному");
    }

    #[test]
    fn long_texts_count_characters() {
        let f = feed();
        assert_eq!(f.long_texts(10).collect::<Vec<_>>(), vec!["hello world", "a long enough text"]);
        let russian = Feed { posts: vec![Post { author: "a".into(), text: "привет".into(), likes: 0 }] };
        assert_eq!(russian.long_texts(6).count(), 1, "шесть символов, хотя двенадцать байт");
    }

    #[test]
    fn top_is_stable_on_ties() {
        let f = feed();
        let ids: Vec<(&str, u32)> = f.top(3).map(|p| (p.author.as_str(), p.likes)).collect();
        assert_eq!(ids, vec![("bob", 9), ("ann", 9), ("ann", 5)], "у bob и второго поста ann поровну: bob раньше в ленте");
        assert_eq!(f.top(100).count(), 4);
        assert_eq!(f.top(0).count(), 0);
    }

    #[test]
    fn totals_accept_any_iterator_of_posts() {
        let f = feed();
        assert_eq!(total_likes(f.posts.iter()), 24);
        assert_eq!(total_likes(f.by_author("ann")), 14);
        assert_eq!(total_likes(f.top(2)), 18);
        assert_eq!(total_likes(std::iter::empty()), 0);
    }
}
