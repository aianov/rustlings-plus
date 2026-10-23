pub struct User {
    pub id: u32,
    pub name: String,
    pub verified: bool,
}

pub enum Body {
    Text(String),
    Photo { url: String, alt: Option<String> },
    Poll(Vec<String>),
    Repost(Box<Post>),
}

pub struct Post {
    pub id: u64,
    pub author: User,
    pub body: Body,
    pub likes: u32,
}

pub fn headline(post: &Post) -> String {
    let Post { author, body, .. } = post;
    let who = if author.verified { format!("{} (verified)", author.name) } else { author.name.clone() };
    match body {
        Body::Text(text) => format!("{who}: {text}"),
        Body::Photo { alt: Some(alt), .. } => format!("{who} posted a photo: {alt}"),
        Body::Photo { alt: None, .. } => format!("{who} posted a photo"),
        Body::Poll(options) => match options.as_slice() {
            [a, b] => format!("{who} asks: {a} or {b}"),
            others => format!("{who} runs a poll with {} options", others.len()),
        },
        Body::Repost(original) => match &**original {
            Post { author: User { name, .. }, body: Body::Text(text), .. } => {
                format!("{who} reposted {name}: {text}")
            }
            Post { author: User { name, .. }, .. } => format!("{who} reposted {name}"),
        },
    }
}

pub fn leaderboard(scores: &[(String, u32)], min: u32) -> Vec<String> {
    scores
        .iter()
        .filter(|(_, score)| *score >= min)
        .enumerate()
        .map(|(place, (name, score))| format!("{}. {name} ({score})", place + 1))
        .collect()
}

fn main() {
    let scores = vec![("ann".to_string(), 90)];
    println!("{:?}", leaderboard(&scores, 0));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(name: &str, verified: bool) -> User {
        User { id: 1, name: name.to_string(), verified }
    }

    fn post(author: User, body: Body) -> Post {
        Post { id: 10, author, body, likes: 0 }
    }

    #[test]
    fn text_posts() {
        assert_eq!(headline(&post(user("ann", false), Body::Text("hi".into()))), "ann: hi");
        assert_eq!(
            headline(&post(user("ann", true), Body::Text("hi".into()))),
            "ann (verified): hi",
            "у подтверждённого автора после имени идёт (verified)"
        );
    }

    #[test]
    fn photos_with_and_without_alt() {
        let with = Body::Photo { url: "u".into(), alt: Some("a cat".into()) };
        let without = Body::Photo { url: "u".into(), alt: None };
        assert_eq!(headline(&post(user("bob", false), with)), "bob posted a photo: a cat");
        assert_eq!(headline(&post(user("bob", false), without)), "bob posted a photo", "без alt двоеточия нет");
    }

    #[test]
    fn polls_depend_on_option_count() {
        let two = Body::Poll(vec!["tea".into(), "coffee".into()]);
        let three = Body::Poll(vec!["a".into(), "b".into(), "c".into()]);
        let none = Body::Poll(vec![]);
        assert_eq!(headline(&post(user("cat", false), two)), "cat asks: tea or coffee");
        assert_eq!(headline(&post(user("cat", false), three)), "cat runs a poll with 3 options");
        assert_eq!(headline(&post(user("cat", false), none)), "cat runs a poll with 0 options", "пустой опрос тоже опрос");
    }

    #[test]
    fn reposts_look_inside() {
        let inner = post(user("dan", true), Body::Text("rust".into()));
        assert_eq!(
            headline(&post(user("eve", false), Body::Repost(Box::new(inner)))),
            "eve reposted dan: rust",
            "у репостнутого автора звание не пишем, только имя"
        );
        let photo = post(user("dan", false), Body::Photo { url: "u".into(), alt: None });
        assert_eq!(headline(&post(user("eve", true), Body::Repost(Box::new(photo)))), "eve (verified) reposted dan");
    }

    #[test]
    fn repost_of_repost_stops_at_one_level() {
        let deepest = post(user("zed", false), Body::Text("t".into()));
        let middle = post(user("dan", false), Body::Repost(Box::new(deepest)));
        assert_eq!(
            headline(&post(user("eve", false), Body::Repost(Box::new(middle)))),
            "eve reposted dan",
            "если внутри репоста не текст, пишем только автора"
        );
    }

    fn table() -> Vec<(String, u32)> {
        [("ann", 90), ("bob", 40), ("cat", 75), ("dan", 10)].iter().map(|(n, s)| (n.to_string(), *s)).collect()
    }

    #[test]
    fn leaderboard_keeps_order_and_numbers_only_kept() {
        assert_eq!(leaderboard(&table(), 50), vec!["1. ann (90)", "2. cat (75)"], "места считаются среди оставшихся");
        assert_eq!(leaderboard(&table(), 0).len(), 4);
        assert_eq!(leaderboard(&table(), 91), Vec::<String>::new());
    }

    #[test]
    fn written_with_patterns() {
        let prod = include_str!("mtch4.rs").split("#[cfg(test)]").next().expect("файл не пуст");
        assert!(prod.contains("..") && (prod.contains("Post {") || prod.contains("User {")) && prod.matches("Body::").count() >= 4,
            "подпись должна собираться образцами по форме данных, включая `..` для лишних полей");
        assert!(prod.contains("|(") || prod.contains("| (") , "в замыканиях параметры должны разбираться образцами вида `|(name, score)|`");
    }

    #[test]
    fn boundary_is_inclusive() {
        assert_eq!(leaderboard(&table(), 75), vec!["1. ann (90)", "2. cat (75)"], "ровно min проходит");
    }
}
