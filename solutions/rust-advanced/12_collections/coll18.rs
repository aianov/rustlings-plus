use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub id: u64,
    pub author: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Like,
    Follow,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Notice {
    pub kind: Kind,
    pub target: u64,
}

pub fn unique_authors(feed: &[Item]) -> Vec<u64> {
    let mut seen = HashSet::new();
    feed.iter().map(|i| i.author).filter(|a| seen.insert(*a)).collect()
}

pub fn hide_muted(feed: &mut Vec<Item>, muted: &HashSet<u64>) {
    feed.retain(|i| !muted.contains(&i.author));
}

pub fn take_page(feed: &mut Vec<Item>, n: usize) -> Vec<Item> {
    let n = n.min(feed.len());
    feed.drain(..n).collect()
}

pub fn collapse_runs(notices: &mut Vec<Notice>) {
    notices.dedup_by_key(|n| (n.kind, n.target));
}

fn main() {
    let feed = vec![Item { id: 1, author: 7 }];
    println!("{:?}", unique_authors(&feed));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: u64, author: u64) -> Item {
        Item { id, author }
    }

    #[test]
    fn authors_appear_once_in_order_of_first_post() {
        let feed = vec![item(1, 7), item(2, 3), item(3, 7), item(4, 9), item(5, 3), item(6, 7)];
        assert_eq!(unique_authors(&feed), vec![7, 3, 9], "повторы не подряд тоже должны уйти");
    }

    #[test]
    fn every_muted_post_disappears() {
        let mut feed = vec![item(1, 5), item(2, 5), item(3, 1), item(4, 5), item(5, 2), item(6, 5), item(7, 5)];
        hide_muted(&mut feed, &HashSet::from([5]));
        let ids: Vec<u64> = feed.iter().map(|i| i.id).collect();
        assert_eq!(ids, vec![3, 5], "после удаления соседний пост сдвигается на место удалённого и не должен пропускаться");
    }

    #[test]
    fn a_page_is_cut_from_the_front() {
        let mut feed: Vec<Item> = (1..=5).map(|i| item(i, 1)).collect();
        let page = take_page(&mut feed, 2);
        assert_eq!(page.iter().map(|i| i.id).collect::<Vec<_>>(), vec![1, 2]);
        assert_eq!(feed.len(), 3);
        let rest = take_page(&mut feed, 10);
        assert_eq!(rest.len(), 3, "просили больше, чем есть: отдаём остаток");
        assert!(feed.is_empty());
        assert!(take_page(&mut feed, 4).is_empty());
    }

    #[test]
    fn runs_of_equal_notices_collapse() {
        let n = |kind, target| Notice { kind, target };
        let mut list = vec![n(Kind::Like, 1), n(Kind::Like, 1), n(Kind::Like, 1), n(Kind::Follow, 1), n(Kind::Like, 1), n(Kind::Like, 2), n(Kind::Like, 2)];
        collapse_runs(&mut list);
        assert_eq!(list, vec![n(Kind::Like, 1), n(Kind::Follow, 1), n(Kind::Like, 1), n(Kind::Like, 2)], "сворачиваются только соседние одинаковые");
    }
}
