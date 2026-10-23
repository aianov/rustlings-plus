// Лента отдаётся страницами по пять постов. Пользователь листает: прочитал первую страницу, а за это время у автора вышло четыре новых
// поста. Вторая страница считается как «пропустить пять, взять пять», но пропустить теперь надо девять, поэтому вторая страница
// начинается с того, что пользователь уже видел: четыре дубля сверху, а старые посты на самом деле съехали вниз. Курсор здесь это
// просто смещение, и он врёт, как только лента меняется. Формат курсора и функции его разбора уже есть (они из `svhl10`).
// Цель: курсор указывает на конкретный пост, а не на позицию. Обход ленты страницами при любых вставках новых постов показывает
// каждый пост, который был до начала обхода, ровно один раз.


fn main() {}


#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Post {
    pub id: u64,
    pub created_ms: u64,
    pub text: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct BadCursor;


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


pub struct Page {
    pub items: Vec<Post>,
    pub next: Option<String>,
}

pub struct Feed {
    posts: Vec<Post>,
}

impl Feed {
    pub fn new() -> Feed {
        Feed { posts: Vec::new() }
    }

    /// Добавляет пост, лента остаётся отсортированной от новых к старым по паре (время, номер).
    pub fn insert(&mut self, post: Post) {
        let at = self.posts.partition_point(|p| (p.created_ms, p.id) > (post.created_ms, post.id));
        self.posts.insert(at, post);
    }

    pub fn page(&self, cursor: Option<&str>, limit: usize) -> Result<Page, BadCursor> {
        let offset: usize = match cursor {
            None => 0,
            Some(c) => c.strip_prefix('o').and_then(|n| n.parse().ok()).ok_or(BadCursor)?,
        };
        let items: Vec<Post> = self.posts.iter().skip(offset).take(limit).cloned().collect();
        let next = (offset + items.len() < self.posts.len()).then(|| format!("o{}", offset + items.len()));
        Ok(Page { items, next })
    }
}

impl Default for Feed {
    fn default() -> Feed {
        Feed::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn post(id: u64) -> Post {
        Post { id, created_ms: id, text: String::new() }
    }

    fn filled(n: u64) -> Feed {
        let mut f = Feed::new();
        for i in 0..n {
            f.insert(post(i));
        }
        f
    }

    #[test]
    fn a_quiet_feed_is_walked_in_full() {
        let f = filled(23);
        let mut seen = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let p = f.page(cursor.as_deref(), 5).unwrap();
            seen.extend(p.items.iter().map(|x| x.id));
            match p.next {
                Some(c) => cursor = Some(c),
                None => break,
            }
        }
        assert_eq!(seen, (0..23).rev().collect::<Vec<_>>());
    }

    #[test]
    fn new_posts_do_not_shift_the_next_page() {
        let mut f = filled(30);
        let first = f.page(None, 5).unwrap();
        assert_eq!(first.items.iter().map(|p| p.id).collect::<Vec<_>>(), vec![29, 28, 27, 26, 25]);
        for id in 100..104 {
            f.insert(post(id));
        }
        let second = f.page(first.next.as_deref(), 5).unwrap();
        assert_eq!(
            second.items.iter().map(|p| p.id).collect::<Vec<_>>(),
            vec![24, 23, 22, 21, 20],
            "пока пользователь читал, вышли новые посты, и вторая страница стала повторять уже виденное"
        );
    }

    #[test]
    fn a_walk_with_inserts_between_every_page_loses_nothing_and_repeats_nothing() {
        let mut f = filled(40);
        let mut seen = Vec::new();
        let mut cursor: Option<String> = None;
        let mut fresh = 1000;
        for _ in 0..100 {
            let p = f.page(cursor.as_deref(), 6).unwrap();
            seen.extend(p.items.iter().map(|x| x.id));
            f.insert(post(fresh));
            f.insert(post(fresh + 1));
            fresh += 2;
            match p.next {
                Some(c) => cursor = Some(c),
                None => break,
            }
        }
        let unique: HashSet<_> = seen.iter().collect();
        assert_eq!(unique.len(), seen.len(), "в обходе есть повторы");
        let old: HashSet<u64> = (0..40).collect();
        assert!(old.iter().all(|id| seen.contains(id)), "пропущен пост, который был в ленте до начала обхода");
    }

    #[test]
    fn a_foreign_or_damaged_cursor_is_refused() {
        let f = filled(10);
        assert!(f.page(Some("o5"), 3).is_err(), "курсор-смещение больше не формат");
        assert!(f.page(Some(""), 3).is_err());
        assert!(f.page(Some(&"0".repeat(36)), 3).is_err(), "контрольная сумма не сошлась");
    }
}
