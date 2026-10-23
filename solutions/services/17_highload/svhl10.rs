
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

fn checksum(body: &str) -> u16 {
    let mut h: u32 = 0x811c9dc5;
    for b in body.bytes() {
        h ^= u32::from(b);
        h = h.wrapping_mul(16777619);
    }
    (h ^ (h >> 16)) as u16
}

pub fn encode_cursor(created_ms: u64, id: u64) -> String {
    let body = format!("{created_ms:016x}{id:016x}");
    format!("{body}{:04x}", checksum(&body))
}

pub fn decode_cursor(s: &str) -> Result<(u64, u64), BadCursor> {
    if s.len() != 36 || !s.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) {
        return Err(BadCursor);
    }
    let (body, sum) = s.split_at(32);
    if u16::from_str_radix(sum, 16).map_err(|_| BadCursor)? != checksum(body) {
        return Err(BadCursor);
    }
    let created = u64::from_str_radix(&body[..16], 16).map_err(|_| BadCursor)?;
    let id = u64::from_str_radix(&body[16..], 16).map_err(|_| BadCursor)?;
    Ok((created, id))
}


pub fn page(posts: &[Post], cursor: Option<&str>, limit: usize) -> Result<Page, BadCursor> {
    let after = cursor.map(decode_cursor).transpose()?;
    let limit = limit.clamp(1, 100);
    let mut rest = posts.iter().filter(|p| after.is_none_or(|(created, id)| (p.created_ms, p.id) < (created, id)));
    let items: Vec<Post> = rest.by_ref().take(limit).cloned().collect();
    let next = if rest.next().is_some() { items.last().map(|p| encode_cursor(p.created_ms, p.id)) } else { None };
    Ok(Page { items, next })
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
