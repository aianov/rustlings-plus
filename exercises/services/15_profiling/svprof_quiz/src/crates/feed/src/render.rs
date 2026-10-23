use crate::storage::Post;

fn escaped(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")
}

pub fn render_page(user: u64, posts: &[Post], limit: usize, out: &mut String) {
    let items: Vec<String> = posts
        .iter()
        .take(limit)
        .map(|post| format!("{{\"id\":{},\"author\":\"{}\",\"text\":\"{}\",\"likes\":{}}}", post.id, escaped(&post.author), escaped(&post.text), post.likes))
        .collect();
    *out = format!("{{\"user\":{},\"posts\":[{}]}}", user, items.join(","));
}
