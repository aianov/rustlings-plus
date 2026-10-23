use std::time::Duration;

use api_gateway::{Claim, Config, Known, ProfileCache};
use contracts::{Changed, FeedPage, PostView, UserId, UserView};
use social_cli::Web;

use crate::support::{Gateway, ok, within};

fn view(id: u64, name: &str) -> UserView {
    UserView::named(UserId(id), name)
}

#[tokio::test(start_paused = true)]
async fn profile_cache_flies_once_remembers_misses_and_obeys_versions() {
    let cache = ProfileCache::sized(Duration::from_secs(30), 2);
    let Claim::Mine(first) = cache.claim(UserId(1)) else {
        panic!("первый запрос летит сам");
    };
    let Claim::Wait(mut waiting) = cache.claim(UserId(1)) else {
        panic!("второй ждёт первого: single-flight");
    };
    cache.finish(first, Some(Known::Found(view(1, "alice"))));
    waiting
        .wait_for(|done| *done)
        .await
        .expect("ожидание кончилось");
    assert_eq!(cache.fresh(UserId(1)), Some(Known::Found(view(1, "alice"))));
    let Claim::Mine(ghost) = cache.claim(UserId(9)) else {
        panic!("свободно");
    };
    cache.finish(ghost, Some(Known::Missing));
    assert_eq!(
        cache.fresh(UserId(9)),
        Some(Known::Missing),
        "промах тоже кэшируется"
    );
    tokio::time::advance(Duration::from_secs(6)).await;
    assert_eq!(cache.fresh(UserId(9)), None, "но коротко");
    let Claim::Mine(slow) = cache.claim(UserId(1)) else {
        panic!("свободно");
    };
    assert!(
        cache.invalidate(UserId(1), 3),
        "событие profile.updated версии 3"
    );
    cache.finish(slow, Some(Known::Found(view(1, "старое имя"))));
    assert_eq!(
        cache.fresh(UserId(1)),
        None,
        "ответ, начатый до инвалидации, не попадает в кэш"
    );
    assert!(
        !cache.invalidate(UserId(1), 2),
        "старая версия события ничего не трогает"
    );
    cache.put(view(2, "bob"));
    cache.put(view(3, "carol"));
    cache.put(view(4, "dave"));
    assert_eq!(cache.len(), 2, "ёмкость держит LRU");
    assert_eq!(cache.stale(UserId(2)), None, "вытеснен самый давний");
    tokio::time::advance(Duration::from_secs(40)).await;
    assert_eq!(cache.fresh(UserId(4)), None);
    assert_eq!(
        cache.stale(UserId(4)),
        Some(view(4, "dave")),
        "протухший остаётся на случай отказа"
    );
}

#[tokio::test]
async fn the_feed_scrolls_by_cursor_and_rejects_a_broken_one() {
    let stack = Gateway::start(Config::default()).await;
    let (alice, mut writer) = stack.member("alice").await;
    let (_, mut reader) = stack.member("bob").await;
    let _ = ok!(reader, "users.follow", format!(r#"{{"target": "{}"}}"#, alice.id) => Changed);
    let mut written = Vec::new();
    for n in 0..7 {
        let post = ok!(writer, "posts.create", format!(r#"{{"text": "пост {n}"}}"#) => PostView);
        written.push(post.id);
    }
    stack
        .feed_until(&mut reader, "семь постов alice у bob", |page| {
            page.posts.len() == 7 || page.next.is_some()
        })
        .await;
    let mut seen = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let data = match &cursor {
            Some(cursor) => format!(r#"{{"size": 3, "cursor": "{cursor}"}}"#),
            None => String::from(r#"{"size": 3}"#),
        };
        let page = ok!(reader, "posts.feed", data => FeedPage);
        assert!(page.posts.len() <= 3);
        seen.extend(page.posts.iter().map(|post| post.id));
        match page.next {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }
    let mut newest_first = written.clone();
    newest_first.reverse();
    assert_eq!(seen, newest_first, "курсор ведёт без пропусков и повторов");
    let broken = reader
        .fails("posts.feed", r#"{"size": 3, "cursor": "c1.zz.1.00000000"}"#)
        .await;
    assert_eq!(
        broken.code, "validation",
        "битый курсор это отказ: {broken:?}"
    );
    stack.stop().await;
}

#[tokio::test]
async fn a_renamed_author_shows_up_fresh_on_every_gateway() {
    let stack = Gateway::start(Config::default()).await;
    let (alice, mut writer) = stack.member("alice").await;
    let (_, mut reader) = stack.member("bob").await;
    let _ = ok!(reader, "users.follow", format!(r#"{{"target": "{}"}}"#, alice.id) => Changed);
    let _ = ok!(writer, "posts.create", r#"{"text": "привет"}"#.to_string() => PostView);
    let page = stack
        .feed_until(&mut reader, "пост alice", |page| !page.posts.is_empty())
        .await;
    assert!(page.authors.iter().any(|author| author.id == alice.id));
    let token = stack.login("alice").await.token;
    let renamed = within(
        "смена имени",
        Web::new(&stack.http.to_string()).request(
            "PATCH",
            "/users/me",
            Some(r#"{"display_name": "Алиса"}"#),
            Some(&token),
        ),
    )
    .await
    .expect("шлюз отвечает");
    assert_eq!(renamed.status, 200, "{renamed:?}");
    stack
        .feed_until(
            &mut reader,
            "новое имя в авторах ленты",
            |page| {
                page.authors.iter().any(|author| {
                    author.id == alice.id && author.display_name.as_deref() == Some("Алиса")
                })
            },
        )
        .await;
    stack.stop().await;
}
