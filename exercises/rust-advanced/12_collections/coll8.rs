// Пагинация по времени: ключ поста это (время, id), страницы идут от новых к старым, а курсор клиент хранит
// как ключ последнего увиденного поста. В тестах вечно ловят одно: на границе страниц то один пост показан
// дважды, то пропал. Рядом лежит «что нового с момента X» для опроса, и там та же боль.
// TODO: Найди, почему граница плывёт, и почини обе функции. Подсказка про тип границы: она бывает
// включённой и исключённой.
use std::collections::BTreeMap;
use std::ops::Bound;

pub type Key = (u64, u64);

pub fn page_before(feed: &BTreeMap<Key, String>, cursor: Option<Key>, n: usize) -> Vec<Key> {
    match cursor {
        Some(c) => feed.range(..=c).rev().take(n).map(|(k, _)| *k).collect(),
        None => feed.keys().rev().take(n).copied().collect(),
    }
}

pub fn newer_than(feed: &BTreeMap<Key, String>, last_seen: Key) -> Vec<Key> {
    feed.range(last_seen..).map(|(k, _)| *k).collect()
}

fn main() {
    let feed = BTreeMap::from([((1, 1), "a".to_string())]);
    println!("{:?}", page_before(&feed, None, 5));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed() -> BTreeMap<Key, String> {
        let stamps = [10, 10, 10, 20, 20, 30, 30, 30, 30, 40];
        stamps.iter().enumerate().map(|(i, ts)| ((*ts, i as u64 + 1), format!("p{i}"))).collect()
    }

    #[test]
    fn pages_do_not_repeat_the_boundary_post() {
        let feed = feed();
        let first = page_before(&feed, None, 4);
        let second = page_before(&feed, first.last().copied(), 4);
        assert_eq!(first, vec![(40, 10), (30, 9), (30, 8), (30, 7)]);
        assert_eq!(second, vec![(30, 6), (20, 5), (20, 4), (10, 3)], "последний пост прошлой страницы второй раз показывать нельзя");
    }

    #[test]
    fn walking_pages_covers_everything_once_even_with_equal_times() {
        let feed = feed();
        for size in 1..=6 {
            let mut seen = Vec::new();
            let mut cursor = None;
            for _ in 0..50 {
                let page = page_before(&feed, cursor, size);
                if page.is_empty() {
                    break;
                }
                cursor = page.last().copied();
                seen.extend(page);
            }
            let mut expected: Vec<Key> = feed.keys().copied().collect();
            expected.reverse();
            assert_eq!(seen, expected, "размер страницы {size}: посты потеряны или повторены");
        }
    }

    #[test]
    fn polling_returns_only_strictly_newer_posts() {
        let feed = feed();
        assert_eq!(newer_than(&feed, (30, 8)), vec![(30, 9), (40, 10)], "пост, который клиент уже видел, не возвращаем");
        assert!(newer_than(&feed, (40, 10)).is_empty(), "ничего нового: пусто, а не последний пост ещё раз");
        assert_eq!(newer_than(&feed, (0, 0)).len(), 10);
    }
}
