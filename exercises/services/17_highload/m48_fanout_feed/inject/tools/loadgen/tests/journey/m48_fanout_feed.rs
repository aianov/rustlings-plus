use std::time::Duration;

use e2e::{Stack, StackConfig};
use loadgen::{Closed, Scenario, run_closed, stack_target};

pub const FEED_P99_MS: f64 = 500.0;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_feed_holds_its_p99_under_load() {
    let stack = Stack::start_with(StackConfig {
        gateway: api_gateway::Config {
            limits: api_gateway::RateLimits {
                user_rate: 1_000_000,
                user_burst: 1_000_000,
                ip_rate: 1_000_000,
                ip_burst: 1_000_000,
                ..api_gateway::RateLimits::default()
            },
            ..api_gateway::Config::default()
        },
        ..StackConfig::default()
    })
    .await;
    let gateway = stack.http.to_string();
    let scenario = Scenario::parse("post,feed,feed,feed").expect("сценарий");
    let hit = stack_target(&gateway, 8, scenario)
        .await
        .expect("пользователи нагрузки готовы");
    let report = run_closed(
        Closed {
            users: 8,
            duration: Duration::from_secs(3),
        },
        hit,
    )
    .await
    .report("лента под нагрузкой");
    assert_eq!(report.errors, 0, "{}", report.render());
    let feed = report
        .steps
        .iter()
        .find(|step| step.name == "feed")
        .expect("шаг ленты в отчёте");
    assert!(feed.requests >= 50, "{}", report.render());
    assert!(
        feed.p99_ms < FEED_P99_MS,
        "p99 ленты {:.1} мс при пороге {FEED_P99_MS} мс:\n{}",
        feed.p99_ms,
        report.render()
    );
    stack.stop().await;
}
