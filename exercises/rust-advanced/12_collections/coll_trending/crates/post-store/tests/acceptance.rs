use post_store::{Page, Store, StoreError};

fn ids(page: &Page) -> Vec<u64> {
    page.posts.clone()
}

#[test]
fn posts_get_sequential_ids_and_are_readable() {
    let mut s = Store::new();
    assert_eq!(s.add_post(7, 100, "hello"), 1);
    assert_eq!(s.add_post(8, 101, "world"), 2);
    let p = s.get(2).expect("пост 2 должен существовать");
    assert_eq!((p.author, p.ts, p.text.as_str()), (8, 101, "world"));
    assert!(s.get(3).is_none());
}

#[test]
fn author_pages_walk_without_repeats_or_gaps() {
    let mut s = Store::new();
    let stamps = [10, 10, 10, 20, 20, 30, 30, 30, 30, 40];
    for ts in stamps {
        s.add_post(7, ts, "post");
    }
    s.add_post(8, 15, "other author");
    for n in 1..=6 {
        let mut seen = Vec::new();
        let mut cursor = None;
        for _ in 0..40 {
            let page = s.author_page(7, cursor, n);
            seen.extend(ids(&page));
            match page.next {
                Some(c) => cursor = Some(c),
                None => break,
            }
        }
        assert_eq!(seen, vec![10, 9, 8, 7, 6, 5, 4, 3, 2, 1], "размер страницы {n}: потеряны или повторены посты, или чужой автор в ленте");
    }
}

#[test]
fn the_last_page_has_no_cursor_and_unknown_authors_are_empty() {
    let mut s = Store::new();
    for ts in 1..=4 {
        s.add_post(1, ts, "x");
    }
    assert_eq!(s.author_page(1, None, 4), Page { posts: vec![4, 3, 2, 1], next: None }, "ровно столько, сколько просили: курсор не нужен");
    assert_eq!(s.author_page(1, None, 3).next, Some((2, 2)));
    assert_eq!(s.author_page(1, Some((1, 1)), 3), Page { posts: vec![], next: None });
    assert_eq!(s.author_page(99, None, 3), Page { posts: vec![], next: None });
    assert_eq!(s.author_page(1, None, 0).posts, Vec::<u64>::new());
}

#[test]
fn tags_follow_edits_and_deletes() {
    let mut s = Store::new();
    let a = s.add_post(1, 1, "learning #Rust and #cats");
    let b = s.add_post(2, 2, "#rust again");
    assert_eq!(s.with_tag("rust"), vec![a, b]);
    assert_eq!(s.with_tag("cats"), vec![a]);
    assert_eq!(s.tags_known(), 2);

    assert!(s.edit(a, "now about #dogs"));
    assert_eq!(s.with_tag("rust"), vec![b], "после правки старый тег должен потерять пост");
    assert!(s.with_tag("cats").is_empty());
    assert_eq!(s.with_tag("dogs"), vec![a]);
    assert_eq!(s.get(a).map(|p| p.text.as_str()), Some("now about #dogs"));

    assert!(s.delete(b));
    assert!(s.with_tag("rust").is_empty());
    assert_eq!(s.tags_known(), 1, "тег без постов должен исчезнуть, остался только dogs");
    assert!(!s.edit(b, "ghost"));
    assert!(!s.delete(b));
}

#[test]
fn a_deleted_post_is_gone_everywhere() {
    let mut s = Store::new();
    let a = s.add_post(1, 10, "keep");
    let b = s.add_post(1, 20, "drop #bye");
    s.like(b, 5, 30).unwrap();
    assert!(s.delete(b));
    assert_eq!(ids(&s.author_page(1, None, 10)), vec![a], "удалённый пост остался на странице автора");
    assert!(s.trending(40, 100, 5).is_empty(), "удалённый пост остался в трендах");
    assert_eq!(s.like_count(b), 0);
    assert_eq!(s.like(b, 6, 31), Err(StoreError::NoSuchPost(b)));
    assert!(s.get(b).is_none());
}

#[test]
fn a_user_likes_a_post_once() {
    let mut s = Store::new();
    let p = s.add_post(1, 1, "x");
    assert_eq!(s.like(p, 10, 5), Ok(true));
    assert_eq!(s.like(p, 10, 6), Ok(false), "второй лайк того же пользователя не считается");
    assert_eq!(s.like(p, 11, 7), Ok(true));
    assert_eq!(s.like_count(p), 2);
    assert_eq!(s.like(99, 10, 5), Err(StoreError::NoSuchPost(99)));
    assert_eq!(StoreError::NoSuchPost(99).to_string(), "post 99 does not exist");
}

#[test]
fn trending_counts_only_the_window() {
    let mut s = Store::new();
    let a = s.add_post(1, 0, "a");
    let b = s.add_post(1, 0, "b");
    let c = s.add_post(1, 0, "c");
    for (user, ts) in [(1, 50), (2, 60), (3, 100)] {
        s.like(a, user, ts).unwrap();
    }
    for (user, ts) in [(1, 90), (2, 95)] {
        s.like(b, user, ts).unwrap();
    }
    s.like(c, 1, 10).unwrap();
    assert_eq!(s.trending(100, 40, 10), vec![(b, 2), (a, 1)], "окно (60, 100]: лайк ровно на границе 60 уже не входит, ровно на now входит");
    assert_eq!(s.trending(100, 50, 10), vec![(a, 2), (b, 2)], "при равенстве лайков раньше меньший id");
    assert_eq!(s.trending(10, 1000, 10), vec![(c, 1)], "окно длиннее истории: вычитание не должно уйти ниже нуля, лайки в момент 0 не входят");
}

#[test]
fn trending_keeps_the_best_n_and_skips_quiet_posts() {
    let mut s = Store::new();
    let posts: Vec<u64> = (0..30).map(|i| s.add_post(1, i, "p")).collect();
    for (i, post) in posts.iter().enumerate() {
        for user in 0..(i as u64 % 7) {
            s.like(*post, user, 100).unwrap();
        }
    }
    let top = s.trending(100, 10, 4);
    assert_eq!(top.len(), 4);
    assert!(top.iter().all(|(_, n)| *n == 6), "четыре лучших поста набрали по 6 лайков: {top:?}");
    assert_eq!(top.iter().map(|(p, _)| *p).collect::<Vec<_>>(), vec![7, 14, 21, 28], "среди равных первыми идут меньшие id");
    assert!(s.trending(100, 10, 0).is_empty());
    assert!(s.trending(500, 10, 5).is_empty(), "в окне нет ни одного лайка: трендов нет");
    let all = s.trending(100, 10, 100);
    assert!(all.iter().all(|(_, n)| *n > 0), "посты без лайков в окне не попадают");
    assert!(all.windows(2).all(|w| w[0].1 >= w[1].1), "результат по убыванию лайков");
}
