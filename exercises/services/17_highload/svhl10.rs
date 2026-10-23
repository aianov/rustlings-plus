// Лента отдаётся страницами. Клиент получает вместе со страницей «курсор», непрозрачную строку, и присылает её за следующей страницей.
// Курсор показывает место, откуда продолжать: пары (время поста, номер). Это не номер страницы: страниц у живой ленты нет, посты всё
// время добавляются сверху. Курсор приходит от клиента, значит, его могли испортить: оборвать, перепутать символы, подсунуть чужую строку.
// Цель: `encode_cursor` и `decode_cursor` (36 символов: 16 на время, 16 на номер, 4 контрольных, всё строчными шестнадцатеричными),
// испорченный курсор даёт `BadCursor`; `page` отдаёт посты строго старше курсора, не больше `limit` (но от 1 до 100), и курсор
// следующей страницы только если есть что показывать дальше.


fn main() {}


#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Post {
    pub id: u64,
    pub created_ms: u64,
    pub text: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct BadCursor;



pub struct Page {
    pub items: Vec<Post>,
    pub next: Option<String>,
}

pub fn encode_cursor(created_ms: u64, id: u64) -> String {
    let _ = (created_ms, id);
    todo!()
}

pub fn decode_cursor(s: &str) -> Result<(u64, u64), BadCursor> {
    let _ = s;
    todo!()
}

/// `posts` отсортированы от новых к старым по паре (время, номер).
pub fn page(posts: &[Post], cursor: Option<&str>, limit: usize) -> Result<Page, BadCursor> {
    let _ = (posts, cursor, limit);
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(n: u64) -> Vec<Post> {
        (0..n).rev().map(|i| Post { id: i, created_ms: 1_000 + i / 3, text: format!("п{i}") }).collect()
    }

    fn walk(posts: &[Post], limit: usize) -> Vec<u64> {
        let mut seen = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..1000 {
            let p = page(posts, cursor.as_deref(), limit).unwrap();
            seen.extend(p.items.iter().map(|x| x.id));
            match p.next {
                Some(c) => cursor = Some(c),
                None => return seen,
            }
        }
        panic!("пагинация не заканчивается");
    }

    #[test]
    fn the_cursor_roundtrips() {
        let c = encode_cursor(1_700_000_000_123, 987_654_321);
        assert_eq!(c.len(), 36);
        assert_eq!(decode_cursor(&c), Ok((1_700_000_000_123, 987_654_321)));
        assert_eq!(decode_cursor(&encode_cursor(u64::MAX, 0)), Ok((u64::MAX, 0)));
    }

    #[test]
    fn a_damaged_cursor_is_refused() {
        let good = encode_cursor(1_700_000_000_123, 987_654_321);
        assert_eq!(decode_cursor(""), Err(BadCursor));
        assert_eq!(decode_cursor(&good[..35]), Err(BadCursor), "оборванный");
        assert_eq!(decode_cursor(&format!("{good}0")), Err(BadCursor), "с хвостом");
        assert_eq!(decode_cursor(&"z".repeat(36)), Err(BadCursor), "не шестнадцатеричный");
        assert_eq!(decode_cursor(&good.to_uppercase()), Err(BadCursor), "чужой формат записи");
        for i in 0..36 {
            let mut bytes = good.clone().into_bytes();
            bytes[i] = if bytes[i] == b'a' { b'b' } else { b'a' };
            assert_eq!(decode_cursor(&String::from_utf8(bytes).unwrap()), Err(BadCursor), "изменён символ {i}: курсор принят");
        }
        let mut swapped = good.clone().into_bytes();
        swapped.swap(5, 6);
        if swapped != good.as_bytes() {
            assert_eq!(decode_cursor(&String::from_utf8(swapped).unwrap()), Err(BadCursor), "соседние символы переставлены: контрольная сумма должна это заметить");
        }
    }

    #[test]
    fn pages_cover_the_feed_without_gaps_or_repeats() {
        let posts = feed(50);
        let seen = walk(&posts, 7);
        let expected: Vec<u64> = posts.iter().map(|p| p.id).collect();
        assert_eq!(seen, expected, "обход по страницам должен дать ленту целиком, по одному разу, в том же порядке");
    }

    #[test]
    fn posts_with_equal_timestamps_are_neither_skipped_nor_doubled() {
        let posts: Vec<Post> = (0..10u64).rev().map(|i| Post { id: i, created_ms: 500, text: String::new() }).collect();
        assert_eq!(walk(&posts, 3), posts.iter().map(|p| p.id).collect::<Vec<_>>(), "у всех одно время: курсор обязан различать по номеру");
    }

    #[test]
    fn the_last_page_has_no_next_cursor() {
        let posts = feed(6);
        assert!(page(&posts, None, 6).unwrap().next.is_none(), "ровно по размеру страницы: дальше ничего нет, курсор не нужен");
        assert!(page(&posts, None, 5).unwrap().next.is_some());
        assert!(page(&[], None, 5).unwrap().items.is_empty());
    }

    #[test]
    fn limit_is_clamped_and_bad_cursor_is_an_error() {
        let posts = feed(300);
        assert_eq!(page(&posts, None, 1000).unwrap().items.len(), 100);
        assert_eq!(page(&posts, None, 0).unwrap().items.len(), 1);
        assert!(page(&posts, Some("мусор"), 10).is_err());
    }
}
