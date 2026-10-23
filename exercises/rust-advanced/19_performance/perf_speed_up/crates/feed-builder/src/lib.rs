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
    let mut visible: Vec<Post> = Vec::new();
    for post in posts {
        if follows.contains(&post.author) && !blocked.contains(&post.author) {
            visible.push(post.clone());
        }
    }
    let mut sorted: Vec<Post> = Vec::new();
    for post in visible {
        let at = sorted.iter().position(|other| other.id < post.id).unwrap_or(sorted.len());
        sorted.insert(at, post);
    }
    let mut cards = Vec::new();
    for post in sorted.into_iter().take(limit) {
        let preview: String = post.text.chars().take(40).collect::<String>().to_uppercase();
        cards.push(Card { id: post.id, title: format!("{}: {}", post.author, preview), likes: post.likes });
    }
    cards
}
