// Монитор сердцебиений. Узлы присылают сигнал `beat(имя)`. Монитор каждые `period` (interval)
// просматривает узлы: если с прошлого просмотра сигнала не было, узлу засчитывается пропуск,
// если был, пропуски обнуляются. Узел мёртв после `max_missed` пропусков подряд; новый сигнал
// оживляет. Пока сигнала не было ни разу, узел Unknown.
//
// Требования:
//  1. `start` запускает фоновую задачу (она останавливается, когда монитор уничтожен, это
//     уже сделано в Drop). Просмотры идут по interval с периодом `period`,
//     первый сразу при старте. Задержка `stall_once` вставляется после тика, перед обходом узлов.
//  2. Если просмотр затянулся (в конфиге `stall_once`: первый просмотр задерживается на столько),
//     пропущенные просмотры нельзя догонять пачкой: это ложно убило бы живые узлы.
//  3. `wait_dead(узел, within)` ждёт, пока узел станет мёртвым, проверяя статус каждые 10 мс,
//     и отвечает Err(WaitError::Timeout), если за `within` этого не случилось.
//  4. `wait_all_dead(узлы, budget)` ждёт смерти всех, причём `budget` общий на всех: он
//     отсчитывается один раз от начала вызова, а не заново для каждого узла.
// TODO: Допиши `start`, `beat`, `status`, `wait_dead`, `wait_all_dead`.
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::task::JoinHandle;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Status {
    Unknown,
    Alive,
    Dead,
}

#[derive(Debug, PartialEq)]
pub enum WaitError {
    Timeout,
}

#[derive(Clone)]
pub struct Config {
    pub period: Duration,
    pub max_missed: u32,
    pub stall_once: Option<Duration>,
}

#[derive(Default)]
struct Node {
    seen: bool,
    missed: u32,
}

type Nodes = Arc<Mutex<HashMap<String, Node>>>;

pub struct Monitor {
    nodes: Nodes,
    config: Config,
    task: JoinHandle<()>,
}

impl Drop for Monitor {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Monitor {
    pub fn start(config: Config) -> Monitor {
        todo!()
    }

    pub fn beat(&self, node: &str) {
        todo!()
    }

    pub fn status(&self, node: &str) -> Status {
        todo!()
    }

    pub async fn wait_dead(&self, node: &str, within: Duration) -> Result<(), WaitError> {
        todo!()
    }

    pub async fn wait_all_dead(&self, nodes: &[&str], budget: Duration) -> Result<(), WaitError> {
        todo!()
    }
}

#[tokio::main]
async fn main() {
    let monitor = Monitor::start(Config { period: Duration::from_millis(100), max_missed: 3, stall_once: None });
    monitor.beat("a");
    println!("{:?}", monitor.status("a"));
    println!("{:?}", monitor.wait_dead("a", Duration::from_secs(1)).await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{Instant, sleep};

    fn config(max_missed: u32, stall_once: Option<Duration>) -> Config {
        Config { period: Duration::from_millis(100), max_missed, stall_once }
    }

    #[tokio::test(start_paused = true)]
    async fn unknown_then_alive() {
        let monitor = Monitor::start(config(3, None));
        assert_eq!(monitor.status("a"), Status::Unknown, "до первого сигнала узел неизвестен");
        monitor.beat("a");
        assert_eq!(monitor.status("a"), Status::Alive, "после сигнала узел жив");
    }

    #[tokio::test(start_paused = true)]
    async fn silent_node_dies_after_max_missed_periods() {
        let monitor = Monitor::start(config(3, None));
        monitor.beat("a");
        let started = Instant::now();
        monitor.wait_dead("a", Duration::from_secs(5)).await.expect("молчащий узел должен умереть");
        let took = started.elapsed();
        assert!(
            took >= Duration::from_millis(300) && took <= Duration::from_millis(320),
            "узел должен умереть через три пропущенных периода (300 мс), а умер через {took:?}"
        );
        monitor.beat("a");
        assert_eq!(monitor.status("a"), Status::Alive, "новый сигнал оживляет узел");
    }

    #[tokio::test(start_paused = true)]
    async fn steady_beats_keep_the_node_alive() {
        let monitor = Monitor::start(config(2, None));
        sleep(Duration::from_millis(50)).await;
        for _ in 0..20 {
            monitor.beat("a");
            sleep(Duration::from_millis(100)).await;
            assert_eq!(monitor.status("a"), Status::Alive, "узел шлёт сигнал каждый период, умирать ему нельзя");
        }
    }

    #[tokio::test(start_paused = true)]
    async fn slow_scan_does_not_kill_live_nodes() {
        let monitor = Monitor::start(config(2, Some(Duration::from_millis(330))));
        sleep(Duration::from_millis(50)).await;
        for step in 0..8 {
            assert_ne!(
                monitor.status("a"),
                Status::Dead,
                "на шаге {step} живой узел объявлен мёртвым: после задержки просмотра пропущенные тики нельзя отдавать пачкой"
            );
            monitor.beat("a");
            sleep(Duration::from_millis(100)).await;
        }
    }

    #[tokio::test(start_paused = true)]
    async fn wait_dead_times_out_for_a_live_node() {
        let monitor = Monitor::start(config(3, None));
        let started = Instant::now();
        let waiter = monitor.wait_dead("a", Duration::from_millis(250));
        let keep_alive = async {
            for _ in 0..10 {
                monitor.beat("a");
                sleep(Duration::from_millis(50)).await;
            }
        };
        let (result, ()) = tokio::join!(waiter, keep_alive);
        assert_eq!(result, Err(WaitError::Timeout), "живой узел не умирает: ожидание должно кончиться таймаутом");
        assert!(started.elapsed() >= Duration::from_millis(500), "keep_alive длится 500 мс");
    }

    #[tokio::test(start_paused = true)]
    async fn budget_is_shared_between_nodes() {
        let monitor = Monitor::start(config(3, None));
        monitor.beat("a");
        monitor.beat("b");
        sleep(Duration::from_millis(250)).await;
        monitor.beat("b");
        let started = Instant::now();
        let result = monitor.wait_all_dead(&["a", "b"], Duration::from_millis(330)).await;
        assert_eq!(
            result,
            Err(WaitError::Timeout),
            "a умирает на 300 мс, b на 600 мс, а общий бюджет в 330 мс (от начала вызова, то есть до 580 мс) вышел раньше. Бюджет общий, а не свой у каждого узла"
        );
        let took = started.elapsed();
        assert!(
            took >= Duration::from_millis(330) && took <= Duration::from_millis(350),
            "ждать нужно ровно бюджет, а ждали {took:?}"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn everybody_dies_within_a_big_budget() {
        let monitor = Monitor::start(config(3, None));
        monitor.beat("a");
        monitor.beat("b");
        monitor.wait_all_dead(&["a", "b"], Duration::from_secs(2)).await.expect("оба молчат и должны умереть");
        assert_eq!(monitor.status("a"), Status::Dead);
        assert_eq!(monitor.status("b"), Status::Dead);
    }
}
