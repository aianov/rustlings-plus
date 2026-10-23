use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use testkit::{eventually, unique_prefix};

const MS: fn(u64) -> Duration = Duration::from_millis;

#[test]
fn prefixes_are_unique_across_threads_and_safe_as_subject_tokens() {
    let handles: Vec<_> = (0..8).map(|_| std::thread::spawn(|| (0..200).map(|_| unique_prefix()).collect::<Vec<_>>())).collect();
    let all: Vec<String> = handles.into_iter().flat_map(|h| h.join().unwrap()).collect();
    let unique: HashSet<&String> = all.iter().collect();
    assert_eq!(unique.len(), all.len(), "два теста с одним префиксом услышат друг друга: префикс должен быть уникален даже при вызовах из многих потоков");
    for prefix in &all {
        assert!(prefix.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'), "префикс служит одним токеном темы NATS: только буквы, цифры и _, а тут {prefix:?}");
        assert!(prefix.chars().next().is_some_and(|c| c.is_ascii_alphabetic()), "и начинается с буквы: {prefix:?}");
    }
}

#[tokio::test]
async fn eventually_notices_a_condition_soon_after_it_turns_true() {
    let began = Instant::now();
    let result = eventually(MS(2000), MS(20), || async move { if began.elapsed() >= MS(100) { Ok("ok") } else { Err("рано".to_string()) } }).await;
    assert_eq!(result, Ok("ok"), "условие стало истинным через 100 мс, срок две секунды");
    assert!(began.elapsed() < MS(600), "ждали {:?}: надо заметить сразу после наступления, а не отсиживать весь срок", began.elapsed());
}

#[tokio::test]
async fn eventually_checks_at_once_and_returns_the_value() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let began = Instant::now();
    let result = eventually(MS(1000), MS(200), move || {
        counter.fetch_add(1, Ordering::SeqCst);
        async { Ok::<u32, String>(42) }
    })
    .await;
    assert_eq!(result, Ok(42), "значение пробы возвращается");
    assert_eq!(calls.load(Ordering::SeqCst), 1, "истинное сразу условие проверяется один раз");
    assert!(began.elapsed() < MS(100), "и без паузы");
}

#[tokio::test]
async fn eventually_reports_the_last_reason_after_the_deadline() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let began = Instant::now();
    let result: Result<(), String> = eventually(MS(300), MS(100), move || {
        let n = counter.fetch_add(1, Ordering::SeqCst) + 1;
        async move { Err(format!("попытка {n}")) }
    })
    .await;
    let message = result.unwrap_err();
    assert!(message.starts_with("не дождались за "), "начало сообщения: {message}");
    assert!(message.contains("попытка"), "в сообщении причина последней неудачи: {message}");
    let took = began.elapsed();
    assert!(took >= MS(300) && took < MS(700), "ждали {took:?}: ровно срок, не меньше и не намного больше");
}

#[tokio::test]
async fn eventually_makes_a_last_check_exactly_at_the_deadline() {
    let began = Instant::now();
    let result = eventually(MS(300), MS(200), || async move { if began.elapsed() >= MS(299) { Ok(()) } else { Err("рано".to_string()) } }).await;
    assert_eq!(result, Ok(()), "условие стало истинным к самому сроку: последняя проверка делается именно в срок, а не по счёту попыток");
}
