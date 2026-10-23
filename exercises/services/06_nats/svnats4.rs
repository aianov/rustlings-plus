// Подписчик NATS называет тему шаблоном: `*` заменяет ровно одно слово между точками, `>` заменяет
// всё остаток, но минимум одно слово, и стоит только в конце. Нужно проверять шаблоны в нашем коде:
// фильтры на шлюзе, тесты расписаний, маршрутизация в свои очереди. Цель: matches отвечает так же,
// как брокер, а шаблоны и темы, которые брокеру показывать нельзя, не совпадают ни с чем.


fn main() {}


pub fn is_valid_subject(subject: &str) -> bool {
    let _ = subject;
    todo!()
}

pub fn is_valid_pattern(pattern: &str) -> bool {
    let _ = pattern;
    todo!()
}

pub fn matches(pattern: &str, subject: &str) -> bool {
    let _ = (pattern, subject);
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_subjects() {
        assert!(matches("social.post.created", "social.post.created"), "одинаковые темы совпадают");
        assert!(!matches("social.post.created", "social.post"), "короче шаблона не совпадает");
        assert!(!matches("social.post.created", "social.post.created.alice"), "длиннее шаблона тоже");
        assert!(!matches("social.post.created", "Social.post.created"), "регистр различается");
    }

    #[test]
    fn star_is_exactly_one_token() {
        assert!(matches("social.*.created", "social.post.created"), "звёздочка в середине");
        assert!(matches("*.*", "a.b"), "две звёздочки подряд");
        assert!(matches("*", "alice"), "одна звёздочка и одно слово");
        assert!(!matches("*", "a.b"), "звёздочка не берёт два слова");
        assert!(!matches("social.*", "social"), "звёздочка не берёт ноль слов");
        assert!(!matches("social.*.created", "social.created"), "и в середине не берёт ноль слов");
        assert!(!matches("social.*", "social.post.created"), "звёздочка не берёт хвост");
    }

    #[test]
    fn tail_wildcard_needs_at_least_one_token() {
        assert!(matches("social.>", "social.post"), "один токен хвоста");
        assert!(matches("social.>", "social.post.liked.alice"), "много токенов хвоста");
        assert!(matches(">", "anything.at.all"), "одна треугольная скобка берёт всё");
        assert!(matches("social.*.>", "social.post.liked"), "звёздочка и хвост вместе");
        assert!(!matches("social.>", "social"), "хвост не берёт ноль слов");
        assert!(!matches("social.>", "notify.post"), "начало должно совпасть");
        assert!(!matches("social.post.>", "social.user.followed"), "и середина тоже");
    }

    #[test]
    fn what_the_broker_would_reject_matches_nothing() {
        for pattern in ["", "a..b", ".a", "a.", "a.>.b", ">.a", "a.b*", "a.*b", "a b", "a.>x", "a\r\nb"] {
            assert!(!is_valid_pattern(pattern), "{pattern:?} не шаблон");
            for subject in ["a.b", "a", "", "a..b"] {
                assert!(!matches(pattern, subject), "негодный шаблон {pattern:?} не совпадает с {subject:?}");
            }
        }
        for subject in ["", "a..b", "a.", ".a", "a.*", "a.>", "*", "a b", "a\tb", "a\r\nb"] {
            assert!(!is_valid_subject(subject), "{subject:?} не тема для публикации");
            assert!(!matches(">", subject), "негодная тема {subject:?} не совпадает даже с `>`");
            assert!(!matches("a.*", subject), "негодная тема {subject:?} не совпадает с `a.*`");
        }
    }

    #[test]
    fn valid_shapes_are_accepted() {
        for pattern in ["a", "a.b", "*", ">", "a.*", "a.>", "*.*.>", "social.post.created.*", "a-b.c_d.9"] {
            assert!(is_valid_pattern(pattern), "{pattern:?} годный шаблон");
        }
        for subject in ["a", "a.b", "social.post.created.alice", "a-b.c_d.9"] {
            assert!(is_valid_subject(subject), "{subject:?} годная тема");
        }
    }
}
