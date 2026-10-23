use std::collections::HashMap;

pub struct Post {
    pub id: u64,
    pub author: u64,
    pub tags: Vec<String>,
}

pub fn ids_by_author(feed: &[Post]) -> HashMap<u64, Vec<u64>> {
    let mut groups: HashMap<u64, Vec<u64>> = HashMap::new();
    for post in feed {
        groups.entry(post.author).or_default().push(post.id);
    }
    groups
}

pub fn tag_counts(feed: &[Post]) -> HashMap<&str, usize> {
    let mut counts = HashMap::new();
    for tag in feed.iter().flat_map(|p| &p.tags) {
        *counts.entry(tag.as_str()).or_insert(0) += 1;
    }
    counts
}

pub fn top_tag<'a>(counts: &HashMap<&'a str, usize>) -> Option<(&'a str, usize)> {
    counts
        .iter()
        .map(|(tag, n)| (*tag, *n))
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(a.0)))
}

fn main() {
    let feed = vec![Post { id: 1, author: 7, tags: vec!["rust".to_string()] }];
    println!("{:?}", ids_by_author(&feed));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn post(id: u64, author: u64, tags: &[&str]) -> Post {
        Post { id, author, tags: tags.iter().map(|t| t.to_string()).collect() }
    }

    fn feed() -> Vec<Post> {
        vec![
            post(10, 1, &["rust", "async"]),
            post(11, 2, &["rust"]),
            post(12, 1, &["cats"]),
            post(13, 3, &[]),
            post(14, 1, &["async", "rust"]),
        ]
    }

    #[test]
    fn posts_are_grouped_in_feed_order() {
        let groups = ids_by_author(&feed());
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[&1], vec![10, 12, 14], "порядок внутри группы как в ленте");
        assert_eq!(groups[&2], vec![11]);
        assert_eq!(groups[&3], vec![13]);
    }

    #[test]
    fn an_empty_feed_has_no_groups() {
        assert!(ids_by_author(&[]).is_empty());
        assert!(tag_counts(&[]).is_empty());
    }

    #[test]
    fn tags_are_counted_without_copying() {
        let posts = feed();
        let counts = tag_counts(&posts);
        assert_eq!(counts["rust"], 3);
        assert_eq!(counts["async"], 2);
        assert_eq!(counts["cats"], 1);
        assert_eq!(counts.len(), 3);
        let key = counts.keys().find(|k| **k == "cats").expect("ключ cats");
        let original = posts[2].tags[0].as_ptr();
        assert_eq!(key.as_ptr(), original, "ключ должен указывать в строку поста, а не в её копию");
    }

    #[test]
    fn the_most_popular_tag_wins_and_ties_go_alphabetically() {
        let posts = feed();
        assert_eq!(top_tag(&tag_counts(&posts)), Some(("rust", 3)));
        let tie = vec![post(1, 1, &["zebra"]), post(2, 1, &["apple"]), post(3, 1, &["mango"])];
        assert_eq!(top_tag(&tag_counts(&tie)), Some(("apple", 1)), "при равенстве побеждает меньший тег по алфавиту");
        assert_eq!(top_tag(&HashMap::new()), None);
    }
}
