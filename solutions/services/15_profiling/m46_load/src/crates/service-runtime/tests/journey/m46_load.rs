use std::time::Duration;

use service_runtime::{
    ALIVE_TASKS, GLOBAL_QUEUE, Metrics, SLOW_POLLS, WORKERS, metrics, record_runtime,
    watch_polls_with,
};

fn slow_polls(task: &str) -> u64 {
    let text = metrics().render();
    let key = format!("{SLOW_POLLS}{{task=\"{task}\"}} ");
    text.lines()
        .find_map(|line| line.strip_prefix(&key))
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

#[tokio::test]
async fn a_blocking_poll_is_reported() {
    let before = slow_polls("m46-blocking");
    watch_polls_with("m46-blocking", Duration::from_millis(5), async {
        let until = std::time::Instant::now() + Duration::from_millis(20);
        let mut spins = 0u64;
        while std::time::Instant::now() < until {
            spins = spins.wrapping_add(1);
            std::hint::spin_loop();
        }
        spins
    })
    .await;
    assert_eq!(
        slow_polls("m46-blocking"),
        before + 1,
        "тяжёлый расчёт в poll замечен"
    );
    watch_polls_with("m46-quick", Duration::from_millis(50), async {
        tokio::time::sleep(Duration::from_millis(80)).await;
    })
    .await;
    assert_eq!(
        slow_polls("m46-quick"),
        0,
        "долгое ожидание это не долгий poll: таймер отдаёт поток"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn runtime_gauges_land_in_metrics() {
    let own = Metrics::new();
    let tasks: Vec<_> = (0..5)
        .map(|_| tokio::spawn(tokio::time::sleep(Duration::from_millis(200))))
        .collect();
    assert!(record_runtime(&own));
    let text = own.render();
    assert!(text.contains(&format!("{WORKERS} 3")), "{text}");
    let alive: i64 = text
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{ALIVE_TASKS} ")))
        .and_then(|value| value.parse().ok())
        .expect("живые задачи в метриках");
    assert!(alive >= 5, "{text}");
    assert!(text.contains(GLOBAL_QUEUE), "{text}");
    for task in tasks {
        let _ = task.await;
    }
}

#[test]
fn without_a_runtime_there_is_nothing_to_record() {
    assert!(!record_runtime(&Metrics::new()));
}
