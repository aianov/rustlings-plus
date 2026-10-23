use std::collections::HashSet;

pub struct Post {
    pub title: String,
    pub author: String,
}

pub fn visible_titles(posts: &[Post], hidden: &[&str]) -> impl Iterator<Item = String> {
    let hidden_set: HashSet<String> = hidden.iter().map(|s| s.to_string()).collect();
    posts.iter().filter(move |p| !hidden_set.contains(&p.author)).map(|p| p.title.clone())
}

pub fn long_words(text: &str, min: usize) -> impl Iterator<Item = String> {
    text.split_whitespace().map(str::to_lowercase).filter(move |w| w.len() >= min)
}

fn main() {
    println!("{}", visible_titles(&[], &[]).count());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_authors_are_skipped() {
        let posts = vec![
            Post { title: "a".into(), author: "ann".into() },
            Post { title: "b".into(), author: "bob".into() },
            Post { title: "c".into(), author: "ann".into() },
        ];
        let got: Vec<String> = visible_titles(&posts, &["bob"]).collect();
        assert_eq!(got, vec!["a", "c"]);
        assert_eq!(visible_titles(&posts, &[]).count(), 3);
        assert_eq!(visible_titles(&posts, &["ann", "bob"]).count(), 0);
    }

    #[test]
    fn the_filter_set_outlives_the_function_call() {
        let posts = vec![Post { title: "a".into(), author: "ann".into() }];
        let (x, ann) = (["x"], ["ann"]);
        let it = visible_titles(&posts, &x);
        let other = visible_titles(&posts, &ann);
        assert_eq!(it.count(), 1);
        assert_eq!(other.count(), 0);
    }

    #[test]
    fn words_are_lowercased_and_filtered() {
        let words: Vec<String> = long_words("Rust IS a Fast language", 4).collect();
        assert_eq!(words, vec!["rust", "fast", "language"]);
        assert_eq!(long_words("", 1).count(), 0);
    }

    #[test]
    fn result_is_owned() {
        let words: Vec<String> = {
            let text = String::from("Hello Big World");
            long_words(&text, 5).collect()
        };
        assert_eq!(words, vec!["hello", "world"]);
    }
}
