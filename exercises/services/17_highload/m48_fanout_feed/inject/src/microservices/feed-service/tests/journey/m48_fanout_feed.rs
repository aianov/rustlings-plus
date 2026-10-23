use std::collections::{BTreeSet, HashMap};

use contracts::{PostId, UserId};
use feed_service::{Change, Cursor, FeedItem, FeedLimits, FeedRepository, MemoryFeed};
use test_support::{Rng, seed};

struct Feed {
    repo: MemoryFeed,
    serial: u64,
}

impl Feed {
    fn new(limits: FeedLimits) -> Feed {
        Feed {
            repo: MemoryFeed::with_limits(limits),
            serial: 0,
        }
    }

    async fn apply(&mut self, change: Change) {
        self.serial += 1;
        let _ = self
            .repo
            .apply(format!("x{}", self.serial), 1, change)
            .await;
    }

    async fn post(&mut self, author: u64, id: u64, at: u64) {
        self.apply(Change::Posted {
            post: PostId(id),
            author: UserId(author),
            author_name: format!("u{author}"),
            text: format!("пост {id}"),
            tags: Vec::new(),
            links: Vec::new(),
            at,
        })
        .await;
    }

    async fn follow(&mut self, who: u64, whom: u64) {
        self.apply(Change::Followed {
            follower: UserId(who),
            followee: UserId(whom),
        })
        .await;
    }

    async fn all(&self, viewer: u64, size: usize) -> Vec<FeedItem> {
        let mut items = Vec::new();
        let mut before = None;
        loop {
            let (page, next) = self.repo.after(UserId(viewer), before, size).await;
            assert!(page.len() <= size);
            items.extend(page);
            match next {
                Some(cursor) => before = Some(cursor),
                None => return items,
            }
        }
    }
}

fn ids(items: &[FeedItem]) -> Vec<u64> {
    items.iter().map(|item| item.post.0).collect()
}

#[test]
fn cursors_are_opaque_and_checked() {
    let cursor = Cursor {
        at: 1_700_000_000_123,
        post: PostId(42),
    };
    let text = cursor.encode();
    assert_eq!(Cursor::decode(&text), Ok(cursor));
    let mut broken = text.clone().into_bytes();
    let last = broken.len() - 1;
    broken[last] = if broken[last] == b'0' { b'1' } else { b'0' };
    let broken = String::from_utf8(broken).expect("ascii");
    for bad in [
        broken.as_str(),
        "",
        "c1.1.2",
        "c2.1.2.00000000",
        "мусор",
        &format!("{text}.x"),
    ] {
        assert!(Cursor::decode(bad).is_err(), "{bad}");
    }
}

#[tokio::test]
async fn feed_matches_a_simple_model_on_random_sequences() {
    let seed = seed();
    for round in 0..10u64 {
        let mut rng = Rng::seeded(seed.wrapping_add(round));
        let mut feed = Feed::new(FeedLimits {
            max: 10_000,
            celebrity: 4,
            inline: 2,
        });
        let mut follows: BTreeSet<(u64, u64)> = BTreeSet::new();
        let mut posts: Vec<(u64, u64, u64)> = Vec::new();
        let mut next = 0u64;
        for step in 0..120u64 {
            let who = rng.below(8) + 1;
            if rng.chance(40) {
                let whom = rng.below(8) + 1;
                if who != whom && follows.insert((who, whom)) {
                    feed.follow(who, whom).await;
                }
            } else {
                next += 1;
                let at = 1_000 + step * 10 + rng.below(5);
                feed.post(who, next, at).await;
                posts.push((next, who, at));
            }
        }
        while feed.repo.drain(3).await > 0 {}
        for viewer in 1..=8u64 {
            let mut expected: Vec<(u64, u64)> = posts
                .iter()
                .filter(|(_, author, _)| *author == viewer || follows.contains(&(viewer, *author)))
                .map(|(id, _, at)| (*at, *id))
                .collect();
            expected.sort_by(|a, b| b.cmp(a));
            let expected: Vec<u64> = expected.into_iter().map(|(_, id)| id).collect();
            let got = ids(&feed.all(viewer, 3).await);
            assert_eq!(
                got, expected,
                "seed {seed}, раунд {round}, читатель {viewer}: лента разошлась с моделью"
            );
        }
    }
}

#[tokio::test]
async fn celebrity_posts_are_mixed_in_at_read_time() {
    let mut feed = Feed::new(FeedLimits {
        max: 100,
        celebrity: 3,
        inline: 10,
    });
    for fan in 2..=5 {
        feed.follow(fan, 1).await;
    }
    feed.post(1, 10, 100).await;
    feed.post(2, 11, 101).await;
    let pending: usize = feed.repo.pending().await;
    assert_eq!(pending, 0, "знаменитость ничего не раскладывает");
    let fan = feed.all(2, 10).await;
    assert_eq!(
        ids(&fan),
        [11, 10],
        "пост знаменитости виден подписчику сразу"
    );
    feed.follow(6, 1).await;
    assert_eq!(
        ids(&feed.all(6, 10).await),
        [10],
        "и новому подписчику тоже"
    );
}

#[tokio::test]
async fn big_fan_outs_go_in_background_batches() {
    let mut feed = Feed::new(FeedLimits {
        max: 100,
        celebrity: 1_000,
        inline: 2,
    });
    for fan in 2..=11 {
        feed.follow(fan, 1).await;
    }
    feed.post(1, 7, 100).await;
    assert_eq!(feed.repo.pending().await, 8, "двоим сразу, восьмерым потом");
    assert_eq!(ids(&feed.all(2, 10).await), [7]);
    assert!(
        feed.all(11, 10).await.is_empty(),
        "до пачки пост ещё не у последнего"
    );
    assert_eq!(feed.repo.drain(5).await, 5);
    assert_eq!(feed.repo.drain(5).await, 3);
    assert_eq!(feed.repo.pending().await, 0);
    assert_eq!(ids(&feed.all(11, 10).await), [7]);
}

#[tokio::test]
async fn timelines_are_trimmed_to_n() {
    let mut feed = Feed::new(FeedLimits {
        max: 3,
        celebrity: 1_000,
        inline: 10,
    });
    feed.follow(2, 1).await;
    for id in 1..=6 {
        feed.post(1, id, 100 + id).await;
    }
    assert_eq!(
        ids(&feed.all(2, 2).await),
        [6, 5, 4],
        "в ленте только последние N"
    );
    let mut seen = HashMap::new();
    let (first, next) = feed.repo.after(UserId(2), None, 2).await;
    for item in &first {
        seen.insert(item.post, ());
    }
    let (second, last) = feed.repo.after(UserId(2), next, 2).await;
    assert_eq!(ids(&second), [4]);
    assert!(last.is_none(), "конец ленты: курсора дальше нет");
}
