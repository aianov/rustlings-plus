use feed_builder::{Card, Post, build_feed};
use std::collections::BTreeSet;

mod counting {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    thread_local! {
        static ALLOCS: Cell<usize> = const { Cell::new(0) };
    }

    pub struct Counting;

    fn bump() {
        let _ = ALLOCS.try_with(|n| n.set(n.get() + 1));
    }

    unsafe impl GlobalAlloc for Counting {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            bump();
            unsafe { System.alloc(layout) }
        }

        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            bump();
            unsafe { System.alloc_zeroed(layout) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            unsafe { System.dealloc(ptr, layout) }
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            bump();
            unsafe { System.realloc(ptr, layout, new_size) }
        }
    }

    pub fn allocations<R>(f: impl FnOnce() -> R) -> (R, usize) {
        let before = ALLOCS.get();
        let result = f();
        (result, ALLOCS.get() - before)
    }
}

#[global_allocator]
static GLOBAL: counting::Counting = counting::Counting;

fn within<T: Send + 'static>(secs: u64, what: &str, f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    match rx.recv_timeout(std::time::Duration::from_secs(secs)) {
        Ok(value) => value,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => panic!("{what}: не уложилось в {secs} с: похоже на квадратичный алгоритм"),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => panic!("{what}: поток упал с паникой"),
    }
}

fn dataset(count: usize, authors: u32) -> Vec<Post> {
    (0..count)
        .map(|i| Post {
            id: ((i as u64) * 0x9E37_79B1) & 0xFFFF_FFFF,
            author: (i as u32).wrapping_mul(2_654_435_761) % authors,
            text: if i % 5 == 0 {
                format!("Привет, мир номер {i}! ßtraße и ещё довольно длинный хвост текста про ленту")
            } else {
                format!("post number {i} with a reasonably long tail of text about the feed")
            },
            likes: (i % 101) as u32,
        })
        .collect()
}

fn oracle(posts: &[Post], follows: &[u32], blocked: &[u32], limit: usize) -> Vec<Card> {
    let follows: BTreeSet<u32> = follows.iter().copied().collect();
    let blocked: BTreeSet<u32> = blocked.iter().copied().collect();
    let mut visible: Vec<&Post> = posts.iter().filter(|p| follows.contains(&p.author) && !blocked.contains(&p.author)).collect();
    visible.sort_by(|a, b| b.id.cmp(&a.id));
    visible
        .into_iter()
        .take(limit)
        .map(|p| Card { id: p.id, title: format!("{}: {}", p.author, p.text.chars().take(40).collect::<String>().to_uppercase()), likes: p.likes })
        .collect()
}

#[test]
fn matches_the_reference_on_small_inputs() {
    let posts = dataset(300, 30);
    let cases: [(Vec<u32>, Vec<u32>, usize); 6] = [
        ((0..30).collect(), vec![], 10),
        ((0..30).step_by(3).collect(), vec![6, 9], 25),
        (vec![1, 1, 2, 2, 3], vec![3], 1_000),
        (vec![], vec![], 10),
        ((0..30).collect(), (0..30).collect(), 10),
        (vec![4, 5, 6], vec![], 0),
    ];
    for (follows, blocked, limit) in cases {
        assert_eq!(
            build_feed(&posts, &follows, &blocked, limit),
            oracle(&posts, &follows, &blocked, limit),
            "лента отличается от эталона: подписки {follows:?}, блок {blocked:?}, лимит {limit}"
        );
    }
}

#[test]
fn titles_use_characters_not_bytes() {
    let posts = vec![Post { id: 1, author: 7, text: "я".repeat(60), likes: 3 }];
    let feed = build_feed(&posts, &[7], &[], 5);
    assert_eq!(feed.len(), 1);
    assert_eq!(feed[0].title, format!("7: {}", "Я".repeat(40)), "в заголовок идут первые 40 символов, переведённые в верхний регистр");
    assert_eq!((feed[0].id, feed[0].likes), (1, 3));
}

#[test]
fn big_input_is_processed_in_time() {
    let result = within(8, "лента из 200 000 постов", || {
        let posts = dataset(200_000, 40_000);
        let follows: Vec<u32> = (0..20_000).map(|n| n * 2).collect();
        let blocked: Vec<u32> = (0..5_000).map(|n| n * 8).collect();
        let fast = build_feed(&posts, &follows, &blocked, 50);
        let reference = oracle(&posts, &follows, &blocked, 50);
        (fast, reference)
    });
    assert_eq!(result.0.len(), 50);
    assert_eq!(result.0, result.1, "быстрая лента должна совпасть с эталоном и на большом объёме");
}

#[test]
fn allocations_do_not_grow_with_the_number_of_posts() {
    let posts = dataset(20_000, 2_000);
    let follows: Vec<u32> = (0..1_000).map(|n| n * 2).collect();
    let blocked: Vec<u32> = (0..100).map(|n| n * 20).collect();
    let (feed, allocs) = counting::allocations(|| build_feed(&posts, &follows, &blocked, 20));
    assert_eq!(feed.len(), 20);
    assert!(allocs <= 100, "{allocs} аллокаций на ленту из 20 карточек: клонировать посты и собирать заголовки для всех не нужно, заголовки нужны только тем, кто попал на страницу");
}

#[test]
fn input_is_left_untouched() {
    let posts = dataset(100, 10);
    let copy = posts.clone();
    let _ = build_feed(&posts, &[1, 2, 3], &[], 5);
    assert_eq!(posts, copy);
}
