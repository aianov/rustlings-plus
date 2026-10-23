// Шлюз даёт мобильному клиенту живой канал: клиент присылает два слова («что смотреть» и «чьё»),
// а шлюз подписывается на получившуюся тему NATS и пересылает всё, что туда придёт. Тестировщик
// прислал в поле «чьё» звёздочку и получил поток событий всех пользователей, а с `post.*` в поле
// «что» увидел чужие лайки. Правила: публичное (новые посты) можно смотреть у кого угодно,
// личное (лайки, комментарии, новые подписчики) только про себя. Цель: watch_pattern строит
// тему подписки, и больше ничего, кроме разрешённого, получить через неё нельзя.


fn main() {}

#[derive(Debug, PartialEq)]
pub enum WatchError {
    BadKind,
    BadTarget,
    Forbidden,
}

pub fn watch_pattern(me: &str, kind: &str, target: &str) -> Result<String, WatchError> {
    let _ = me;
    Ok(format!("social.{kind}.{target}"))
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
    fn honest_requests_work() {
        assert_eq!(watch_pattern("alice", "post.created", "bob"), Ok("social.post.created.bob".to_string()), "новые посты bob смотрит кто угодно");
        assert_eq!(watch_pattern("alice", "post.created", "alice"), Ok("social.post.created.alice".to_string()), "и свои");
        for kind in ["post.liked", "post.commented", "user.followed"] {
            let wanted = format!("social.{kind}.alice");
            assert_eq!(watch_pattern("alice", kind, "alice"), Ok(wanted), "{kind} про себя можно");
        }
        assert!(watch_pattern("a_b", "post.liked", "a_b").is_ok(), "подчёркивание в нике");
    }

    #[test]
    fn private_events_of_other_people_are_forbidden() {
        for kind in ["post.liked", "post.commented", "user.followed"] {
            assert_eq!(watch_pattern("alice", kind, "bob"), Err(WatchError::Forbidden), "{kind} bob чужое");
        }
        assert_eq!(watch_pattern("alice", "post.liked", "Alice"), Err(WatchError::Forbidden), "ник сравнивается как есть, Alice и alice разные");
    }

    #[test]
    fn wildcards_and_extra_dots_in_the_kind_are_rejected() {
        for kind in ["*.*", "post.*", "*", ">", "post.>", "post", "post.created.bob", "post.created.*", "POST.CREATED", "", "post.liked ", "social.post.created"] {
            assert_eq!(watch_pattern("alice", kind, "alice"), Err(WatchError::BadKind), "kind {kind:?} не из списка разрешённых");
        }
    }

    #[test]
    fn hostile_targets_never_widen_the_subscription() {
        for target in ["*", ">", "bob.>", "bob.*", "a.b", "", "a b", "bob\r\nPUB x 1", "ник", "bob>", &"x".repeat(33)] {
            assert_eq!(watch_pattern("alice", "post.created", target), Err(WatchError::BadTarget), "target {target:?} не ник");
            assert!(watch_pattern("alice", "post.liked", target).is_err(), "и для личных событий {target:?} не проходит");
        }
    }

    #[test]
    fn the_result_selects_one_users_events_only() {
        let pattern = watch_pattern("alice", "post.created", "bob").unwrap();
        assert!(matches(&pattern, "social.post.created.bob"), "посты bob приходят");
        for other in ["social.post.created.alice", "social.post.created.bobby", "social.post.liked.bob", "social.post.created.bob.x"] {
            assert!(!matches(&pattern, other), "через подписку на посты bob не должно быть видно {other}");
        }
    }
}
