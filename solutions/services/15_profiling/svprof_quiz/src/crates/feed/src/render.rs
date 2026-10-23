use crate::storage::Post;
use std::fmt::Write;

fn write_escaped(out: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
}

pub fn render_page(user: u64, posts: &[Post], limit: usize, out: &mut String) {
    out.clear();
    let _ = write!(out, "{{\"user\":{user},\"posts\":[");
    for (i, post) in posts.iter().take(limit).enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "{{\"id\":{},\"author\":\"", post.id);
        write_escaped(out, &post.author);
        out.push_str("\",\"text\":\"");
        write_escaped(out, &post.text);
        let _ = write!(out, "\",\"likes\":{}}}", post.likes);
    }
    out.push_str("]}");
}
