use std::collections::HashSet;
use std::fmt::Write;

#[derive(Debug, Clone, PartialEq)]
pub struct Post {
    pub id: u64,
    pub author: u32,
    pub text: String,
    pub likes: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Card {
    pub id: u64,
    pub title: String,
    pub likes: u32,
}

pub fn build_feed(posts: &[Post], follows: &[u32], blocked: &[u32], limit: usize) -> Vec<Card> {
    let follows: HashSet<u32> = follows.iter().copied().collect();
    let blocked: HashSet<u32> = blocked.iter().copied().collect();
    let mut visible: Vec<&Post> = posts.iter().filter(|post| follows.contains(&post.author) && !blocked.contains(&post.author)).collect();
    let top = limit.min(visible.len());
    if top < visible.len() {
        visible.select_nth_unstable_by(top, |a, b| b.id.cmp(&a.id));
        visible.truncate(top);
    }
    visible.sort_unstable_by(|a, b| b.id.cmp(&a.id));
    let mut cards = Vec::with_capacity(visible.len());
    for post in visible {
        let mut title = String::with_capacity(64);
        let _ = write!(title, "{}: ", post.author);
        title.extend(post.text.chars().take(40).flat_map(char::to_uppercase));
        cards.push(Card { id: post.id, title, likes: post.likes });
    }
    cards
}
