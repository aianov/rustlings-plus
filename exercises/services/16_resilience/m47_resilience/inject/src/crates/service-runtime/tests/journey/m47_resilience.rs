use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use service_runtime::{
    Breaker, BreakerState, Buckets, Bulkhead, Deadline, RetryBudget, RetryPolicy, full_jitter,
    retry,
};
use tokio::time::{Instant, advance};

#[tokio::test(start_paused = true)]
async fn deadline_counts_down_and_reserves_own_work() {
    let deadline = Deadline::within(Duration::from_millis(300));
    advance(Duration::from_millis(100)).await;
    assert_eq!(deadline.left(), Duration::from_millis(200));
    let downstream = deadline.reserve(Duration::from_millis(50));
    assert_eq!(
        downstream.left(),
        Duration::from_millis(150),
        "сосед получает остаток минус своё"
    );
    assert_eq!(
        deadline.cap(Duration::from_millis(80)).left(),
        Duration::from_millis(80)
    );
    advance(Duration::from_millis(250)).await;
    assert!(deadline.expired());
    assert_eq!(
        deadline.reserve(Duration::from_secs(1)).left(),
        Duration::ZERO
    );
}

#[test]
fn full_jitter_stays_under_the_growing_ceiling() {
    let policy = RetryPolicy {
        attempts: 5,
        base: Duration::from_millis(10),
        cap: Duration::from_millis(100),
    };
    for retry in 0..6u32 {
        let ceiling = Duration::from_millis((10u64 << retry).min(100));
        for random in [0u64, 1, 12_345, u64::MAX] {
            assert!(
                full_jitter(&policy, retry, random) <= ceiling,
                "повтор {retry}"
            );
        }
    }
    assert_eq!(
        full_jitter(&policy, 0, 0),
        Duration::ZERO,
        "полный jitter может не ждать вовсе"
    );
}

#[tokio::test(start_paused = true)]
async fn retries_only_what_is_worth_retrying() {
    let policy = RetryPolicy::default();
    let budget = RetryBudget::new(0.1, 10.0);
    let calls = Arc::new(AtomicU32::new(0));
    let counted = Arc::clone(&calls);
    let got: Result<u32, &str> = retry(
        &policy,
        &budget,
        None,
        |e| *e == "unavailable",
        |attempt| {
            counted.fetch_add(1, Ordering::SeqCst);
            async move {
                if attempt < 2 {
                    Err("unavailable")
                } else {
                    Ok(attempt)
                }
            }
        },
    )
    .await;
    assert_eq!(got, Ok(2));
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    let calls = Arc::new(AtomicU32::new(0));
    let counted = Arc::clone(&calls);
    let got: Result<(), &str> = retry(
        &policy,
        &budget,
        None,
        |e| *e == "unavailable",
        |_| {
            counted.fetch_add(1, Ordering::SeqCst);
            async { Err("invalid") }
        },
    )
    .await;
    assert_eq!(got, Err("invalid"));
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "ошибку клиента не повторяем"
    );
}

#[tokio::test(start_paused = true)]
async fn the_retry_budget_stops_a_retry_storm() {
    let policy = RetryPolicy {
        attempts: 10,
        ..RetryPolicy::default()
    };
    let budget = RetryBudget::new(0.1, 3.0);
    let calls = Arc::new(AtomicU32::new(0));
    for _ in 0..5 {
        let counted = Arc::clone(&calls);
        let _: Result<(), &str> = retry(
            &policy,
            &budget,
            None,
            |_| true,
            |_| {
                counted.fetch_add(1, Ordering::SeqCst);
                async { Err("down") }
            },
        )
        .await;
    }
    let total = calls.load(Ordering::SeqCst);
    assert!(
        total <= 5 + 4,
        "пять вызовов и не больше трёх-четырёх повторов на всех: {total}"
    );
    assert!(budget.balance() < 1.0);
}

#[tokio::test(start_paused = true)]
async fn retries_do_not_outlive_the_deadline() {
    let policy = RetryPolicy {
        attempts: 10,
        base: Duration::from_millis(100),
        cap: Duration::from_secs(1),
    };
    let budget = RetryBudget::new(1.0, 100.0);
    let deadline = Deadline::within(Duration::from_millis(150));
    let started = Instant::now();
    let _: Result<(), &str> = retry(
        &policy,
        &budget,
        Some(deadline),
        |_| true,
        |_| async { Err("down") },
    )
    .await;
    assert!(
        started.elapsed() <= Duration::from_millis(150),
        "{:?}",
        started.elapsed()
    );
}

#[tokio::test(start_paused = true)]
async fn breaker_opens_then_lets_one_probe_through() {
    let breaker = Breaker::new(3, Duration::from_secs(5));
    for _ in 0..3 {
        breaker.allow().expect("замкнут").failure();
    }
    assert!(matches!(breaker.state(), BreakerState::Open { .. }));
    let rejected = breaker.allow().expect_err("разомкнут");
    assert_eq!(rejected.retry_after, Duration::from_secs(5));
    advance(Duration::from_secs(5)).await;
    assert_eq!(breaker.state(), BreakerState::HalfOpen { probing: false });
    let probe = breaker.allow().expect("одна проба");
    assert!(breaker.allow().is_err(), "вторая проба ждёт исхода первой");
    probe.failure();
    assert!(
        matches!(breaker.state(), BreakerState::Open { .. }),
        "проба не удалась: снова разомкнут"
    );
    advance(Duration::from_secs(5)).await;
    breaker.allow().expect("новая проба").success();
    assert_eq!(breaker.state(), BreakerState::Closed { failures: 0 });
    let forgotten = breaker.allow().expect("замкнут");
    drop(forgotten);
    assert_eq!(
        breaker.state(),
        BreakerState::Closed { failures: 1 },
        "брошенный пропуск это отказ"
    );
}

#[test]
fn bulkhead_rejects_at_once_when_full() {
    let bulkhead = Bulkhead::new(2);
    let first = bulkhead.try_enter().expect("место есть");
    let _second = bulkhead.try_enter().expect("место есть");
    assert_eq!(bulkhead.try_enter().expect_err("мест нет").limit, 2);
    assert_eq!(bulkhead.in_use(), 2);
    drop(first);
    assert!(bulkhead.try_enter().is_ok(), "место освободилось");
}

#[tokio::test(start_paused = true)]
async fn token_buckets_refill_and_forget_idle_keys() {
    let buckets: Buckets<&str> = Buckets::new(2.0, 3.0);
    for _ in 0..3 {
        buckets.take(&"alice").expect("в ведре три жетона");
    }
    let wait = buckets.take(&"alice").expect_err("ведро пусто");
    assert_eq!(
        wait,
        Duration::from_millis(500),
        "жетон через полсекунды при двух в секунду"
    );
    buckets.take(&"bob").expect("у bob своё ведро");
    advance(Duration::from_millis(500)).await;
    buckets.take(&"alice").expect("жетон натёк");
    assert_eq!(buckets.len(), 2);
    advance(Duration::from_secs(10)).await;
    assert_eq!(
        buckets.evict_idle(Duration::from_secs(5), Instant::now()),
        2,
        "полные и тихие вёдра выброшены"
    );
    assert!(buckets.is_empty());
}
