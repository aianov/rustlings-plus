use std::time::Duration;
use tokio::time::{Instant, MissedTickBehavior, interval};

#[derive(Clone, Copy, Debug)]
enum Job {
    Billing,
    Heartbeat,
    Cleanup,
}

async fn observe(behavior: MissedTickBehavior) -> Vec<u64> {
    let started = Instant::now();
    let mut ticker = interval(Duration::from_millis(100));
    ticker.set_missed_tick_behavior(behavior);
    let mut seen = Vec::new();
    for round in 0..9 {
        ticker.tick().await;
        seen.push(started.elapsed().as_millis() as u64);
        if round == 2 {
            tokio::time::sleep(Duration::from_millis(350)).await;
        }
    }
    seen
}

fn behavior_for(job: Job) -> MissedTickBehavior {
    match job {
        Job::Billing => MissedTickBehavior::Burst,
        Job::Heartbeat => MissedTickBehavior::Skip,
        Job::Cleanup => MissedTickBehavior::Delay,
    }
}

#[tokio::main]
async fn main() {
    for job in [Job::Billing, Job::Heartbeat, Job::Cleanup] {
        println!("{job:?}: {:?}", observe(behavior_for(job)).await);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn billing_loses_nothing() {
        let seen = observe(behavior_for(Job::Billing)).await;
        assert_eq!(
            *seen.last().unwrap(),
            800,
            "расписание билинга должно догнать пропущенные тики и закончить на 800 мс, а вышло {seen:?}"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn heartbeat_has_no_bursts_and_returns_to_the_grid() {
        let seen = observe(behavior_for(Job::Heartbeat)).await;
        assert!(
            seen.windows(2).all(|pair| pair[0] < pair[1]),
            "два сигнала в один момент: пачка недопустима, {seen:?}"
        );
        assert!(
            seen[4..].iter().all(|moment| moment % 100 == 0),
            "после задержки сигналы должны вернуться на сетку 100 мс, а вышло {seen:?}"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn cleanup_never_runs_closer_than_a_period() {
        let seen = observe(behavior_for(Job::Cleanup)).await;
        assert!(
            seen.windows(2).all(|pair| pair[1] - pair[0] >= 100),
            "между запусками уборки должно быть не меньше периода, а вышло {seen:?}"
        );
    }
}
