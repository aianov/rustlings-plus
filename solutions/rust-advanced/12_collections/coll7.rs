use std::collections::BTreeMap;

#[derive(Debug, PartialEq)]
pub struct Page {
    pub ids: Vec<u64>,
    pub next: Option<u64>,
}

pub fn page(posts: &BTreeMap<u64, String>, cursor: Option<u64>, n: usize) -> Page {
    let mut ids: Vec<u64> = match cursor {
        Some(c) => posts.range(..c).rev().take(n + 1).map(|(id, _)| *id).collect(),
        None => posts.keys().rev().take(n + 1).copied().collect(),
    };
    let more = ids.len() > n;
    ids.truncate(n);
    let next = if more { ids.last().copied() } else { None };
    Page { ids, next }
}

fn main() {
    let posts = BTreeMap::from([(1, "a".to_string())]);
    println!("{:?}", page(&posts, None, 10));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn posts(n: u64) -> BTreeMap<u64, String> {
        (1..=n).map(|i| (i * 10, format!("post {i}"))).collect()
    }

    #[test]
    fn the_first_page_is_the_newest() {
        let p = page(&posts(10), None, 3);
        assert_eq!(p.ids, vec![100, 90, 80]);
        assert_eq!(p.next, Some(80), "курсор это id последнего отданного поста");
    }

    #[test]
    fn the_cursor_is_exclusive() {
        let p = page(&posts(10), Some(80), 3);
        assert_eq!(p.ids, vec![70, 60, 50], "пост с id курсора уже был на прошлой странице");
    }

    #[test]
    fn the_last_page_has_no_cursor() {
        let all = posts(6);
        assert_eq!(page(&all, Some(30), 5), Page { ids: vec![20, 10], next: None });
        assert_eq!(page(&all, None, 6), Page { ids: vec![60, 50, 40, 30, 20, 10], next: None }, "ровно столько, сколько просили: ещё страниц нет");
        assert_eq!(page(&all, None, 7).next, None);
        assert_eq!(page(&all, Some(10), 3), Page { ids: vec![], next: None });
    }

    #[test]
    fn walking_all_pages_gives_every_post_once() {
        let all = posts(23);
        let mut seen = Vec::new();
        let mut cursor = None;
        for _ in 0..100 {
            let p = page(&all, cursor, 4);
            seen.extend(p.ids);
            match p.next {
                Some(c) => cursor = Some(c),
                None => break,
            }
        }
        let mut expected: Vec<u64> = all.keys().copied().collect();
        expected.reverse();
        assert_eq!(seen, expected, "обход страниц должен отдать каждый пост ровно один раз");
    }

    #[test]
    fn new_posts_do_not_shift_old_pages() {
        let mut all = posts(10);
        let first = page(&all, None, 3);
        all.insert(500, "fresh".to_string());
        let second = page(&all, first.next, 3);
        assert_eq!(second.ids, vec![70, 60, 50], "пока читали первую страницу, сверху появился пост: вторая не должна съехать");
    }
}
