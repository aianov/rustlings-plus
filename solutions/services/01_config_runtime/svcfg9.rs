use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;
use tokio::task::JoinHandle;
use tokio::time::{Instant, sleep};

pub struct State {
    pub ready: AtomicBool,
    pub accepting: AtomicBool,
    pub in_flight: AtomicUsize,
}

pub struct RequestGuard(Arc<State>);

impl Drop for RequestGuard {
    fn drop(&mut self) {
        self.0.in_flight.fetch_sub(1, Ordering::SeqCst);
    }
}

pub fn begin_request(state: &Arc<State>) -> Option<RequestGuard> {
    if !state.accepting.load(Ordering::SeqCst) {
        return None;
    }
    state.in_flight.fetch_add(1, Ordering::SeqCst);
    Some(RequestGuard(Arc::clone(state)))
}

#[derive(Debug, PartialEq)]
pub struct Outcome {
    pub drained: bool,
}

pub struct Lifecycle {
    state: Arc<State>,
    background: Vec<JoinHandle<()>>,
}

impl Lifecycle {
    pub fn new(background: Vec<JoinHandle<()>>) -> Lifecycle {
        let state = State {
            ready: AtomicBool::new(true),
            accepting: AtomicBool::new(true),
            in_flight: AtomicUsize::new(0),
        };
        Lifecycle { state: Arc::new(state), background }
    }

    pub fn state(&self) -> Arc<State> {
        Arc::clone(&self.state)
    }

    pub async fn shutdown(self, grace: Duration, drain_deadline: Duration) -> Outcome {
        self.state.ready.store(false, Ordering::SeqCst);
        sleep(grace).await;
        self.state.accepting.store(false, Ordering::SeqCst);
        let deadline = Instant::now() + drain_deadline;
        let drained = loop {
            if self.state.in_flight.load(Ordering::SeqCst) == 0 {
                break true;
            }
            if Instant::now() >= deadline {
                break false;
            }
            sleep(Duration::from_millis(5)).await;
        };
        for handle in &self.background {
            handle.abort();
        }
        for handle in self.background {
            let _ = handle.await;
        }
        Outcome { drained }
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    const GRACE: Duration = Duration::from_millis(100);

    #[derive(Debug, Clone, Copy)]
    struct Snapshot {
        ready: bool,
        accepting: bool,
        in_flight: usize,
        ticks: usize,
    }

    fn background(ticks: &Arc<AtomicUsize>) -> JoinHandle<()> {
        let ticks = Arc::clone(ticks);
        tokio::spawn(async move {
            loop {
                ticks.fetch_add(1, Ordering::SeqCst);
                sleep(Duration::from_millis(10)).await;
            }
        })
    }

    #[tokio::test(start_paused = true)]
    async fn phases_happen_in_the_right_order() {
        let ticks = Arc::new(AtomicUsize::new(0));
        let lifecycle = Lifecycle::new(vec![background(&ticks)]);
        let state = lifecycle.state();
        let guard = begin_request(&state).expect("пока работаем, запросы принимаются");
        tokio::spawn(async move {
            sleep(Duration::from_millis(300)).await;
            drop(guard);
        });

        let timeline = Arc::new(Mutex::new(Vec::<Snapshot>::new()));
        let done = Arc::new(AtomicBool::new(false));
        let probe = {
            let (state, ticks, timeline, done) =
                (Arc::clone(&state), Arc::clone(&ticks), Arc::clone(&timeline), Arc::clone(&done));
            tokio::spawn(async move {
                while !done.load(Ordering::SeqCst) {
                    timeline.lock().unwrap().push(Snapshot {
                        ready: state.ready.load(Ordering::SeqCst),
                        accepting: state.accepting.load(Ordering::SeqCst),
                        in_flight: state.in_flight.load(Ordering::SeqCst),
                        ticks: ticks.load(Ordering::SeqCst),
                    });
                    sleep(Duration::from_millis(10)).await;
                }
            })
        };

        sleep(Duration::from_millis(50)).await;
        let outcome = lifecycle.shutdown(GRACE, Duration::from_secs(5)).await;
        let frozen = ticks.load(Ordering::SeqCst);
        done.store(true, Ordering::SeqCst);
        probe.await.unwrap();

        assert_eq!(outcome, Outcome { drained: true }, "запрос закончился раньше дедлайна: drained = true");
        let timeline = timeline.lock().unwrap().clone();
        assert!(
            timeline.iter().all(|s| !s.ready || s.accepting),
            "сначала нужно снять ready, и только потом перестать принимать: {timeline:?}"
        );
        assert!(
            timeline.iter().any(|s| !s.ready && s.accepting),
            "между «не готов» и «не принимаю» должна быть пауза grace: балансировщик должен успеть заметить"
        );
        let drain: Vec<_> = timeline.iter().filter(|s| !s.accepting && s.in_flight > 0).collect();
        assert!(!drain.is_empty(), "должен быть период, когда новые не принимаются, а текущий запрос ещё идёт");
        assert!(
            drain.last().unwrap().ticks > drain.first().unwrap().ticks,
            "фоновая задача должна работать, пока идёт дренаж запросов: останавливать её рано"
        );
        sleep(Duration::from_millis(200)).await;
        assert_eq!(ticks.load(Ordering::SeqCst), frozen, "после shutdown фоновая задача остановлена");
    }

    #[tokio::test(start_paused = true)]
    async fn stuck_request_does_not_block_forever() {
        let ticks = Arc::new(AtomicUsize::new(0));
        let lifecycle = Lifecycle::new(vec![background(&ticks)]);
        let state = lifecycle.state();
        let _stuck = begin_request(&state).unwrap();
        let started = Instant::now();
        let outcome = lifecycle.shutdown(GRACE, Duration::from_millis(200)).await;
        let took = started.elapsed();
        assert_eq!(outcome, Outcome { drained: false }, "запрос завис: drained = false");
        assert!(took >= Duration::from_millis(300), "должны были дождаться grace и дедлайн, а прошло {took:?}");
        assert!(took < Duration::from_millis(400), "но не дольше: grace + дедлайн, а прошло {took:?}");
    }

    #[tokio::test(start_paused = true)]
    async fn new_requests_are_refused_during_shutdown() {
        let lifecycle = Lifecycle::new(vec![]);
        let state = lifecycle.state();
        let task = tokio::spawn(lifecycle.shutdown(GRACE, Duration::from_secs(1)));
        sleep(GRACE + Duration::from_millis(20)).await;
        assert!(begin_request(&state).is_none(), "после grace новые запросы не принимаются");
        assert!(!state.ready.load(Ordering::SeqCst), "и готовность снята");
        task.await.unwrap();
    }
}
