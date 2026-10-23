// Лента постов и десять вопросов к ней. Каждый вопрос это потребитель, который останавливается сразу,
// как только знает ответ (`find`, `position`, `any`), или обходит всё (`count`, `last`). Внимание
// к равным значениям: `max_by_key` при равенстве отдаёт последнего, `min_by_key` первого, а в условиях
// сказано, кого брать.
// TODO: Реализуй функции.

pub struct Post {
    pub id: u32,
    pub author: &'static str,
    pub likes: u32,
    pub text: &'static str,
}

// Первый пост, в тексте которого есть слово целиком (по границам пробелов).
pub fn first_with_word<'a>(posts: &'a [Post], word: &str) -> Option<&'a Post> {
    todo!()
}

// id первого поста, у которого лайков больше лимита.
pub fn first_id_over(posts: &[Post], limit: u32) -> Option<u32> {
    todo!()
}

// Номер первого поста автора.
pub fn position_of_author(posts: &[Post], author: &str) -> Option<usize> {
    todo!()
}

// У каждого поста есть лайк (для пустой ленты верно).
pub fn all_liked(posts: &[Post]) -> bool {
    todo!()
}

// Пост с наибольшим числом лайков; при равенстве самый ранний.
pub fn most_liked(posts: &[Post]) -> Option<&Post> {
    todo!()
}

// Пост с наименьшим числом лайков; при равенстве самый ранний.
pub fn least_liked(posts: &[Post]) -> Option<&Post> {
    todo!()
}

// Третий пост из переданного итератора, если он есть.
pub fn third<'a>(posts: impl Iterator<Item = &'a Post>) -> Option<&'a Post> {
    todo!()
}

// Автор последнего поста.
pub fn last_author(posts: &[Post]) -> Option<&'static str> {
    todo!()
}

// Сколько постов у автора.
pub fn count_by(posts: &[Post], author: &str) -> usize {
    todo!()
}

fn main() {
    println!("{}", count_by(&[], "ann"));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed() -> Vec<Post> {
        vec![
            Post { id: 10, author: "ann", likes: 5, text: "rust is fun" },
            Post { id: 11, author: "bob", likes: 9, text: "trust me" },
            Post { id: 12, author: "ann", likes: 9, text: "fun with iterators" },
            Post { id: 13, author: "cat", likes: 0, text: "" },
        ]
    }

    #[test]
    fn find_family() {
        let f = feed();
        assert_eq!(first_with_word(&f, "rust").map(|p| p.id), Some(10), "слово целиком: в trust слова rust нет");
        assert_eq!(first_with_word(&f, "fun").map(|p| p.id), Some(10));
        assert!(first_with_word(&f, "go").is_none());
        assert_eq!(first_id_over(&f, 5), Some(11));
        assert_eq!(first_id_over(&f, 9), None);
        assert_eq!(position_of_author(&f, "ann"), Some(0));
        assert_eq!(position_of_author(&f, "dan"), None);
    }

    #[test]
    fn all_is_true_for_empty() {
        assert!(!all_liked(&feed()), "у последнего поста нет лайков");
        assert!(all_liked(&feed()[..3]));
        assert!(all_liked(&[]), "в пустой ленте нет поста без лайков");
    }

    #[test]
    fn extremes_prefer_the_earliest_on_ties() {
        let f = feed();
        assert_eq!(most_liked(&f).map(|p| p.id), Some(11), "у постов 11 и 12 по девять лайков, нужен ранний");
        assert_eq!(least_liked(&f).map(|p| p.id), Some(13));
        let tied = [Post { id: 1, author: "a", likes: 2, text: "" }, Post { id: 2, author: "a", likes: 2, text: "" }];
        assert_eq!(least_liked(&tied).map(|p| p.id), Some(1));
        assert!(most_liked(&[]).is_none());
    }

    #[test]
    fn positional_and_counting() {
        let f = feed();
        assert_eq!(third(f.iter()).map(|p| p.id), Some(12));
        assert!(third(f[..2].iter()).is_none());
        assert_eq!(last_author(&f), Some("cat"));
        assert_eq!(last_author(&[]), None);
        assert_eq!(count_by(&f, "ann"), 2);
        assert_eq!(count_by(&f, "zed"), 0);
    }
}
