use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use test_support::{Eventually, Fault, Faults, Rng, post, unique_name, unique_root, user};

#[tokio::test(start_paused = true)]
async fn eventually_retries_with_growing_pauses() {
    let tries = Arc::new(AtomicU32::new(0));
    let started = tokio::time::Instant::now();
    let value = Eventually::default()
        .until("третья попытка", || {
            let tries = Arc::clone(&tries);
            async move {
                let n = tries.fetch_add(1, Ordering::SeqCst) + 1;
                if n >= 3 { Ok(n) } else { Err(n) }
            }
        })
        .await
        .expect("сбылось на третьей");
    assert_eq!(value, 3);
    assert_eq!(
        started.elapsed(),
        Duration::from_millis(15),
        "паузы 5 и 10 мс: растут вдвое"
    );
}

#[tokio::test(start_paused = true)]
async fn eventually_gives_up_with_the_last_answer() {
    let started = tokio::time::Instant::now();
    let gave_up = Eventually::within(Duration::from_secs(1))
        .until("никогда", || async {
            Err::<(), _>("всё ещё нет")
        })
        .await
        .expect_err("условие не сбывается");
    assert_eq!(
        started.elapsed(),
        Duration::from_secs(1),
        "сдаётся ровно в срок"
    );
    assert_eq!(gave_up.last, "\"всё ещё нет\"");
    assert!(gave_up.attempts > 3, "{gave_up}");
    assert!(gave_up.to_string().starts_with("никогда: не сбылось"));
}

#[test]
fn names_and_roots_are_unique() {
    let names: BTreeSet<String> = (0..1000).map(|_| unique_name("alice")).collect();
    assert_eq!(names.len(), 1000);
    assert!(names.iter().all(|name| {
        name.starts_with("alice")
            && name.len() <= 32
            && name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    }));
    assert_ne!(unique_root("e2e"), unique_root("e2e"));
    assert!(unique_name("!!!").starts_with("user"));
}

#[test]
fn builders_name_only_what_matters() {
    let carol = user()
        .named("carol")
        .display_name("Кэрол \"К\"")
        .bio("строка\nвторая");
    assert!(carol.username.starts_with("carol"));
    assert_eq!(carol.password, test_support::PASSWORD);
    assert_eq!(
        carol.credentials(),
        format!(
            "{{\"username\": \"{}\", \"password\": \"correct-horse\"}}",
            carol.username
        )
    );
    assert_eq!(
        carol.profile_patch().as_deref(),
        Some("{\"display_name\": \"Кэрол \\\"К\\\"\", \"bio\": \"строка\\nвторая\"}")
    );
    assert_eq!(user().profile_patch(), None);
    let first = post();
    let second = post();
    assert_ne!(first.text, second.text, "у каждого поста свой текст");
    assert_eq!(
        post().text("привет").tagged("rust").body(),
        "{\"text\": \"привет #rust\"}"
    );
}

#[test]
fn faults_follow_their_schedule() {
    let next = Faults::new(Fault::Next(2));
    assert_eq!(
        (0..4).map(|_| next.hit()).collect::<Vec<_>>(),
        [true, true, false, false]
    );
    let calls = Faults::calls_failing(&[2, 4]);
    assert_eq!(
        (0..5).map(|_| calls.hit()).collect::<Vec<_>>(),
        [false, true, false, true, false]
    );
    let every = Faults::new(Fault::Every(3));
    assert_eq!((0..6).filter(|_| every.hit()).count(), 2);
    let shared = Faults::new(Fault::Always);
    let copy = shared.clone();
    assert!(copy.hit());
    shared.heal();
    assert!(!copy.hit(), "клоны видят одно расписание");
    assert_eq!((shared.calls(), shared.failed()), (2, 1));
}

#[test]
fn rng_repeats_for_the_same_seed() {
    let mut left = Rng::seeded(42);
    let mut right = Rng::seeded(42);
    let a: Vec<u64> = (0..100).map(|_| left.next_u64()).collect();
    let b: Vec<u64> = (0..100).map(|_| right.next_u64()).collect();
    assert_eq!(
        a, b,
        "один seed, одна последовательность: падение повторяется"
    );
    let mut other = Rng::seeded(43);
    assert_ne!(a[0], other.next_u64());
    let mut rng = Rng::seeded(7);
    assert!((0..1000).all(|_| rng.below(6) < 6));
    assert_eq!(rng.below(0), 0);
    let heads = (0..10_000).filter(|_| rng.chance(50)).count();
    assert!((4_000..6_000).contains(&heads), "{heads}");
}
