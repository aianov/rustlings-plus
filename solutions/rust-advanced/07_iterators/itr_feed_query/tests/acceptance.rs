use feed_query::*;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::collections::HashSet;

struct Counting;

thread_local! {
    static ALLOCS: Cell<usize> = const { Cell::new(0) };
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCS.with(|c| c.set(c.get() + 1));
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCS.with(|c| c.set(c.get() + 1));
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static COUNTING: Counting = Counting;

fn allocations<R>(f: impl FnOnce() -> R) -> (R, usize) {
    let before = ALLOCS.with(Cell::get);
    let result = f();
    (result, ALLOCS.with(Cell::get) - before)
}

fn post(id: u64, author: &str, deleted: bool) -> Post {
    Post { id, author: author.to_string(), text: format!("post {id}"), created: id * 10, deleted }
}

fn sample() -> Feed {
    Feed::new(vec![
        post(1, "ann", false),
        post(2, "bob", false),
        post(3, "ann", true),
        post(4, "cat", false),
        post(5, "bob", false),
        post(6, "ann", false),
    ])
}

fn ids<'a>(posts: impl IntoIterator<Item = &'a Post>) -> Vec<u64> {
    posts.into_iter().map(|p| p.id).collect()
}

fn hide(names: &[&str]) -> HashSet<String> {
    names.iter().map(|n| n.to_string()).collect()
}

#[test]
fn feed_is_stored_oldest_first_whatever_the_input_order() {
    let feed = Feed::new(vec![post(3, "a", false), post(1, "a", false), post(2, "a", false)]);
    assert_eq!(ids(feed.visible(&hide(&[]))), vec![3, 2, 1], "новые посты первыми, даже если на вход пришли вперемешку");
    assert_eq!(feed.len(), 3);
}

#[test]
fn visible_skips_deleted_and_hidden_authors() {
    let feed = sample();
    assert_eq!(ids(feed.visible(&hide(&[]))), vec![6, 5, 4, 2, 1], "удалённый пост 3 не виден");
    assert_eq!(ids(feed.visible(&hide(&["bob", "cat"]))), vec![6, 1]);
    assert_eq!(feed.visible(&hide(&["ann", "bob", "cat"])).count(), 0);
}

#[test]
fn visible_builds_no_intermediate_collection() {
    let feed = sample();
    let hidden = hide(&["bob"]);
    let (first, allocs) = allocations(|| feed.visible(&hidden).next().map(|p| p.id));
    assert_eq!(first, Some(6));
    assert_eq!(allocs, 0, "первый видимый пост находится без единой аллокации: промежуточных векторов быть не должно");
    let big = Feed::new((0..50_000).map(|i| post(i, "x", false)).collect());
    let none = hide(&[]);
    let (top, allocs) = allocations(|| big.visible(&none).take(3).map(|p| p.id).sum::<u64>());
    assert_eq!(top, 49_999 + 49_998 + 49_997);
    assert_eq!(allocs, 0, "три поста из пятидесяти тысяч, и ни одной аллокации");
}

#[test]
fn since_stops_at_older_posts() {
    let feed = sample();
    assert_eq!(ids(feed.since(&hide(&[]), 30)), vec![6, 5, 4], "после метки времени 30: пост 3 удалён, а 4, 5, 6 видны");
    assert_eq!(ids(feed.since(&hide(&["bob"]), 0)), vec![6, 4, 1]);
    assert_eq!(feed.since(&hide(&[]), 1000).count(), 0);
}

#[test]
fn authors_are_distinct_in_order_of_newest_post() {
    let feed = sample();
    let none = hide(&[]);
    let authors: Vec<&str> = feed.authors(&none).collect();
    assert_eq!(authors, vec!["ann", "bob", "cat"], "ann написала самый новый видимый пост");
    let no_ann = hide(&["ann"]);
    let authors: Vec<&str> = feed.authors(&no_ann).collect();
    assert_eq!(authors, vec!["bob", "cat"]);
}

#[test]
fn the_pages_adapter_works_on_any_iterator() {
    let got: Vec<Vec<u32>> = (1..=7).pages(3).collect();
    assert_eq!(got, vec![vec![1, 2, 3], vec![4, 5, 6], vec![7]], "неполная последняя страница отдаётся");
    assert_eq!((1..=6).pages(3).count(), 2, "пустой страницы в конце быть не должно");
    assert_eq!(std::iter::empty::<u8>().pages(2).count(), 0);
    let lazy: Vec<Vec<u64>> = (1..).pages(2).take(2).collect();
    assert_eq!(lazy, vec![vec![1, 2], vec![3, 4]], "бесконечный источник не читается целиком");
}

#[test]
#[should_panic(expected = "page size must be positive")]
fn zero_page_size_is_rejected() {
    let _ = (1..3).pages(0);
}

#[test]
fn feed_pages() {
    let feed = sample();
    let hidden = hide(&[]);
    assert_eq!(feed.page(&hidden, 0, 2).map(ids), Some(vec![6, 5]));
    assert_eq!(feed.page(&hidden, 1, 2).map(ids), Some(vec![4, 2]));
    assert_eq!(feed.page(&hidden, 2, 2).map(ids), Some(vec![1]), "последняя страница неполная");
    assert_eq!(feed.page(&hidden, 3, 2), None, "за концом страниц нет");
    let all: Vec<Vec<u64>> = feed.paged(&hide(&["cat"]), 2).map(ids).collect();
    assert_eq!(all, vec![vec![6, 5], vec![2, 1]]);
}

#[test]
fn building_a_page_allocates_only_the_page() {
    let feed = Feed::new((0..50_000).map(|i| post(i, if i % 2 == 0 { "ann" } else { "bob" }, false)).collect());
    let hidden = hide(&["bob"]);
    let (page, allocs) = allocations(|| feed.page(&hidden, 0, 3).map(ids));
    assert_eq!(page, Some(vec![49_998, 49_996, 49_994]));
    assert!(allocs <= 2, "аллокаций {allocs}: страница должна собираться из ленивой цепочки, без промежуточного списка всей ленты");
}

#[test]
fn deep_pages_do_not_pay_for_skipped_ones() {
    let feed = Feed::new((0..50_000).map(|i| post(i, "ann", false)).collect());
    let hidden = hide(&[]);
    let (page, allocs) = allocations(|| feed.page(&hidden, 10_000, 3).map(ids));
    assert_eq!(page, Some(vec![19_999, 19_998, 19_997]));
    assert!(allocs <= 2, "аллокаций {allocs}: пропущенные страницы нельзя собирать в векторы, пропускай элементы через skip");
    assert_eq!(feed.page(&hidden, usize::MAX, 3), None, "смещение не помещается в usize: страницы нет, паники быть не должно");
}

#[test]
fn empty_feed_has_no_pages() {
    let feed = Feed::new(vec![]);
    assert!(feed.is_empty());
    assert_eq!(feed.paged(&hide(&[]), 5).count(), 0);
    assert_eq!(feed.page(&hide(&[]), 0, 5), None);
}
