fn main() {}

#[derive(Debug, PartialEq)]
pub enum SubjectError {
    Empty,
    TooLong,
    BadChar(char),
}

pub const MAX_TOKEN: usize = 32;

/// Проверяет одно слово темы: 1..=32 символа из `A-Za-z0-9_-`.
pub fn token(raw: &str) -> Result<&str, SubjectError> {
    if raw.is_empty() {
        return Err(SubjectError::Empty);
    }
    if raw.chars().count() > MAX_TOKEN {
        return Err(SubjectError::TooLong);
    }
    match raw.chars().find(|c| !(c.is_ascii_alphanumeric() || *c == '_' || *c == '-')) {
        Some(bad) => Err(SubjectError::BadChar(bad)),
        None => Ok(raw),
    }
}

pub fn user_followed(target: &str) -> Result<String, SubjectError> {
    Ok(format!("social.user.followed.{}", token(target)?))
}

pub fn post_created(author: &str) -> Result<String, SubjectError> {
    Ok(format!("social.post.created.{}", token(author)?))
}

pub fn post_liked(author: &str) -> Result<String, SubjectError> {
    Ok(format!("social.post.liked.{}", token(author)?))
}

pub fn post_commented(author: &str) -> Result<String, SubjectError> {
    Ok(format!("social.post.commented.{}", token(author)?))
}

pub fn unread_of(user: &str) -> Result<String, SubjectError> {
    Ok(format!("notify.unread.{}", token(user)?))
}

/// Шаблон для feed-service: только новые посты, от всех авторов.
pub fn feed_pattern() -> String {
    "social.post.created.*".to_string()
}

/// Шаблон для аудита: все события соцсети, какие только будут.
pub fn audit_pattern() -> String {
    "social.>".to_string()
}

/// Шаблон для живого канала пользователя: любое событие, адресованное ему, и больше ничего.
pub fn inbox_pattern(user: &str) -> Result<String, SubjectError> {
    Ok(format!("social.*.*.{}", token(user)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matches(pattern: &str, subject: &str) -> bool {
        let mut want = pattern.split('.');
        let mut have = subject.split('.');
        loop {
            match (want.next(), have.next()) {
                (Some(">"), Some(_)) => return true,
                (Some("*"), Some(_)) => {}
                (Some(w), Some(h)) if w == h => {}
                (None, None) => return true,
                _ => return false,
            }
        }
    }
    #[test]
    fn constructors_follow_the_schema() {
        assert_eq!(user_followed("bob").unwrap(), "social.user.followed.bob", "подписка на bob");
        assert_eq!(post_created("alice").unwrap(), "social.post.created.alice", "новый пост alice");
        assert_eq!(post_liked("alice").unwrap(), "social.post.liked.alice", "лайк поста alice");
        assert_eq!(post_commented("alice").unwrap(), "social.post.commented.alice", "комментарий к посту alice");
        assert_eq!(unread_of("bob").unwrap(), "notify.unread.bob", "запрос непрочитанных");
        assert_eq!(post_created("a_b-9").unwrap(), "social.post.created.a_b-9", "подчёркивание, дефис и цифры годятся");
    }

    #[test]
    fn a_token_must_stay_one_harmless_word() {
        assert_eq!(token(""), Err(SubjectError::Empty), "пустой ник");
        assert_eq!(token("a.b"), Err(SubjectError::BadChar('.')), "точка превратит один ник в два слова темы");
        assert_eq!(token("*"), Err(SubjectError::BadChar('*')), "звёздочка");
        assert_eq!(token(">"), Err(SubjectError::BadChar('>')), "треугольная скобка");
        assert_eq!(token("a b"), Err(SubjectError::BadChar(' ')), "пробел разорвёт строку протокола");
        assert_eq!(token("a\r\nPUB"), Err(SubjectError::BadChar('\r')), "перевод строки позволил бы вставить в протокол чужую команду");
        assert_eq!(token("ник"), Err(SubjectError::BadChar('н')), "только латиница");
        assert_eq!(token(&"x".repeat(32)), Ok("xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"), "32 символа ещё можно");
        assert_eq!(token(&"x".repeat(33)), Err(SubjectError::TooLong), "33 уже много");
        assert!(post_liked("bob.>").is_err() && unread_of("*").is_err() && user_followed("").is_err(), "конструкторы не пропускают плохой ник");
        assert!(inbox_pattern("a.*").is_err(), "и шаблон строится только из хорошего ника");
    }

    #[test]
    fn feed_sees_only_new_posts_of_everybody() {
        let feed = feed_pattern();
        assert!(matches(&feed, &post_created("alice").unwrap()), "feed получает новые посты");
        assert!(matches(&feed, &post_created("bob").unwrap()), "и от другого автора тоже");
        for other in [post_liked("alice").unwrap(), post_commented("alice").unwrap(), user_followed("alice").unwrap(), unread_of("alice").unwrap()] {
            assert!(!matches(&feed, &other), "feed не должен получать {other}");
        }
    }

    #[test]
    fn audit_sees_every_social_event_but_not_other_traffic() {
        let audit = audit_pattern();
        for event in [post_created("a").unwrap(), post_liked("a").unwrap(), post_commented("a").unwrap(), user_followed("a").unwrap()] {
            assert!(matches(&audit, &event), "аудит должен получать {event}");
        }
        assert!(matches(&audit, "social.something.new.never_seen_before"), "и события, которых ещё нет в схеме");
        assert!(!matches(&audit, &unread_of("a").unwrap()), "запросы к notify-service это не события соцсети");
    }

    #[test]
    fn inbox_is_exactly_one_users_events() {
        let alice = inbox_pattern("alice").unwrap();
        for event in [post_liked("alice").unwrap(), post_commented("alice").unwrap(), user_followed("alice").unwrap(), post_created("alice").unwrap()] {
            assert!(matches(&alice, &event), "alice должна получать {event}");
        }
        for stranger in ["bob", "alic", "alicex", "xalice", "Alice"] {
            for event in [post_liked(stranger).unwrap(), user_followed(stranger).unwrap()] {
                assert!(!matches(&alice, &event), "alice не должна получать чужое событие {event}");
            }
        }
        assert!(!matches(&alice, "social.post.liked.alice.extra"), "и события с лишним словом в конце");
        assert!(!matches(&alice, &unread_of("alice").unwrap()), "и запросы к notify-service");
    }
}
