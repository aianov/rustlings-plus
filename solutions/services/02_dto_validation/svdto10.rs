pub struct LinkDto {
    pub url: String,
    pub title: Option<String>,
}

pub struct AuthorRef {
    pub id: u64,
    pub handle: String,
}

pub struct OptionDto {
    pub label: String,
}

pub struct PollDto {
    pub options: Vec<OptionDto>,
}

pub struct PostDto {
    pub text: String,
    pub author: AuthorRef,
    pub links: Vec<LinkDto>,
    pub poll: Option<PollDto>,
}

pub fn validate_post(post: &PostDto) -> Vec<(String, &'static str)> {
    let mut errors = Vec::new();
    check_chars(&mut errors, "text", &post.text, 1, 280);
    if post.author.id == 0 {
        errors.push(("author.id".to_string(), "required"));
    }
    check_chars(&mut errors, "author.handle", &post.author.handle, 3, 20);
    if post.links.len() > 5 {
        errors.push(("links".to_string(), "too_many"));
    }
    for (i, link) in post.links.iter().enumerate() {
        let path = format!("links[{i}]");
        let url = link.url.trim();
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            errors.push((format!("{path}.url"), "invalid_url"));
        } else if url.chars().count() > 200 {
            errors.push((format!("{path}.url"), "too_long"));
        }
        if let Some(title) = &link.title {
            check_chars(&mut errors, &format!("{path}.title"), title, 0, 50);
        }
    }
    if let Some(poll) = &post.poll {
        if !(2..=4).contains(&poll.options.len()) {
            errors.push(("poll.options".to_string(), "bad_count"));
        }
        for (i, option) in poll.options.iter().enumerate() {
            check_chars(&mut errors, &format!("poll.options[{i}].label"), &option.label, 1, 40);
        }
    }
    errors
}

fn check_chars(errors: &mut Vec<(String, &'static str)>, path: &str, raw: &str, min: usize, max: usize) {
    let len = raw.trim().chars().count();
    if len < min {
        errors.push((path.to_string(), "too_short"));
    } else if len > max {
        errors.push((path.to_string(), "too_long"));
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn good() -> PostDto {
        PostDto {
            text: "привет".to_string(),
            author: AuthorRef { id: 1, handle: "alice".to_string() },
            links: vec![LinkDto { url: "https://example.com".to_string(), title: Some("пример".to_string()) }],
            poll: None,
        }
    }

    fn paths(errors: &[(String, &'static str)]) -> Vec<String> {
        errors.iter().map(|(p, _)| p.clone()).collect()
    }

    #[test]
    fn valid_post_has_no_errors() {
        assert!(validate_post(&good()).is_empty(), "правильный пост не должен давать ошибок");
    }

    #[test]
    fn upper_bounds_are_enforced_exactly() {
        let mut post = good();
        post.text = "я".repeat(280);
        post.author.handle = "h".repeat(20);
        post.links = (0..5).map(|_| LinkDto { url: format!("https://e.io/{}", "a".repeat(185)), title: Some("т".repeat(50)) }).collect();
        post.poll = Some(PollDto { options: vec![OptionDto { label: "о".repeat(40) }, OptionDto { label: "б".to_string() }] });
        assert!(validate_post(&post).is_empty(), "ровно на границе всё допустимо: {:?}", validate_post(&post));
        post.text.push('я');
        post.author.handle.push('h');
        post.links.push(LinkDto { url: "https://ok.io".to_string(), title: None });
        post.links[0].title = Some("т".repeat(51));
        post.links[1].url = format!("https://e.io/{}", "a".repeat(190));
        post.poll.as_mut().unwrap().options[0].label.push('о');
        let got = validate_post(&post);
        let want = [
            ("text", "too_long"),
            ("author.handle", "too_long"),
            ("links", "too_many"),
            ("links[0].title", "too_long"),
            ("links[1].url", "too_long"),
            ("poll.options[0].label", "too_long"),
        ];
        assert_eq!(
            got,
            want.iter().map(|(p, c)| (p.to_string(), *c)).collect::<Vec<_>>(),
            "на единицу выше границы каждое правило должно сработать"
        );
    }

    #[test]
    fn top_level_and_author_paths() {
        let post = PostDto { text: "  ".to_string(), author: AuthorRef { id: 0, handle: "al".to_string() }, ..good() };
        assert_eq!(
            validate_post(&post),
            vec![
                ("text".to_string(), "too_short"),
                ("author.id".to_string(), "required"),
                ("author.handle".to_string(), "too_short"),
            ],
            "пути верхнего уровня и вложенного автора, в порядке полей"
        );
    }

    #[test]
    fn list_elements_get_indexed_paths() {
        let mut post = good();
        post.links = vec![
            LinkDto { url: "https://ok.example".to_string(), title: None },
            LinkDto { url: "ftp://nope".to_string(), title: None },
            LinkDto { url: "https://ok.example".to_string(), title: Some("т".repeat(51)) },
        ];
        assert_eq!(
            validate_post(&post),
            vec![("links[1].url".to_string(), "invalid_url"), ("links[2].title".to_string(), "too_long")],
            "индекс идёт с нуля и стоит в квадратных скобках перед именем поля"
        );
    }

    #[test]
    fn too_many_links_are_reported_on_the_list_itself() {
        let mut post = good();
        post.links = (0..6).map(|i| LinkDto { url: format!("https://e{i}.example"), title: None }).collect();
        assert_eq!(validate_post(&post), vec![("links".to_string(), "too_many")], "шесть ссылок: ошибка на самом списке");
    }

    #[test]
    fn poll_is_nested_two_levels_deep() {
        let mut post = good();
        post.poll = Some(PollDto { options: vec![OptionDto { label: "да".to_string() }, OptionDto { label: " ".to_string() }] });
        assert_eq!(
            paths(&validate_post(&post)),
            vec!["poll.options[1].label".to_string()],
            "путь через два уровня вложенности: poll.options[1].label"
        );
        post.poll = Some(PollDto { options: vec![OptionDto { label: "один".to_string() }] });
        assert_eq!(validate_post(&post), vec![("poll.options".to_string(), "bad_count")], "в опросе от двух до четырёх вариантов");
    }

    #[test]
    fn everything_is_reported_in_struct_order() {
        let post = PostDto {
            text: String::new(),
            author: AuthorRef { id: 1, handle: "alice".to_string() },
            links: vec![LinkDto { url: "x".to_string(), title: None }],
            poll: Some(PollDto { options: vec![OptionDto { label: String::new() }, OptionDto { label: "б".to_string() }] }),
        };
        assert_eq!(
            paths(&validate_post(&post)),
            vec!["text".to_string(), "links[0].url".to_string(), "poll.options[0].label".to_string()],
            "порядок: text, author, links, poll"
        );
    }
}
