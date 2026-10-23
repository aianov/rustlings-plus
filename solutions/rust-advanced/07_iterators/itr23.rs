#[derive(Clone, Debug, PartialEq)]
pub struct Post {
    pub text: String,
    pub likes: u32,
}

pub fn promote(posts: &mut Vec<Post>) {
    let hot: Vec<Post> = posts
        .iter()
        .filter(|p| p.likes > 100)
        .map(|p| Post { text: format!("[hot] {}", p.text), likes: 0 })
        .collect();
    posts.extend(hot);
}

pub fn clean(texts: &[&str]) -> Vec<String> {
    texts.iter().map(|t| t.trim()).filter(|t| !t.is_empty()).map(|t| t.to_lowercase()).collect()
}

fn main() {
    println!("{:?}", clean(&[" A ", ""]));
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = include_str!("itr23.rs");

    fn post(text: &str, likes: u32) -> Post {
        Post { text: text.into(), likes }
    }

    #[test]
    fn hot_copies_go_to_the_end_in_order() {
        let mut posts = vec![post("a", 500), post("b", 3), post("c", 101)];
        promote(&mut posts);
        assert_eq!(posts, vec![post("a", 500), post("b", 3), post("c", 101), post("[hot] a", 0), post("[hot] c", 0)]);
    }

    #[test]
    fn promote_without_hits_changes_nothing() {
        let mut posts = vec![post("a", 100)];
        promote(&mut posts);
        assert_eq!(posts.len(), 1, "ровно сто лайков это ещё не популярный");
        let mut none: Vec<Post> = Vec::new();
        promote(&mut none);
        assert!(none.is_empty());
    }

    #[test]
    fn clean_trims_filters_and_lowercases() {
        assert_eq!(clean(&["  Hello ", "", "   ", "WORLD"]), vec!["hello", "world"]);
        assert!(clean(&[]).is_empty());
    }

    #[test]
    fn clean_is_a_single_chain() {
        let code = SRC.split("#[cfg(test)]").next().unwrap_or("");
        let body = code.split("pub fn clean").nth(1).and_then(|rest| rest.split("\n}\n").next()).unwrap_or("");
        let body: String = body.lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
        assert_eq!(body.matches("collect").count(), 1, "в clean должен быть один collect, в самом конце цепочки");
    }
}
