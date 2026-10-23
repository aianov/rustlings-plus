// Остановить связку сложнее, чем запустить. Если убить NATS первым, все, кто на него опирается, начнут сыпать ошибками соединения и теряя то,
// что не успели отправить. Порядок обратный запуску: сначала те, кого никто не ждёт, потом те, кто им нужен. Каждому сервису даётся вежливый
// сигнал SIGTERM и время закончить дела; если он не уложился, его добивают SIGKILL, иначе один зависший сервис остановит выключение всех.
// Цель: `down` останавливает сервис только после того, как остановились все его зависимые (независимые одновременно), опрашивает процесс каждые 50 мс
// и добивает через `grace` после SIGTERM. Отчёт содержит порядок остановки и список добитых.


use futures::future::join_all;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::watch;

fn main() {}

#[derive(Debug, Clone)]
pub struct Service {
    pub name: &'static str,
    pub deps: Vec<&'static str>,
}

#[allow(async_fn_in_trait)]
pub trait Proc {
    /// Просьба завершиться (SIGTERM).
    async fn term(&self);
    async fn exited(&self) -> bool;
    /// Принудительное завершение (SIGKILL).
    async fn kill(&self);
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// Имена в порядке, в котором им отправили SIGTERM.
    pub order: Vec<String>,
    pub killed: Vec<String>,
}

pub async fn down<P: Proc>(services: &[Service], procs: &HashMap<&'static str, P>, grace: Duration) -> Report {
    let _ = grace;
    let report = Mutex::new(Report::default());
    for s in services {
        procs[s.name].kill().await;
        report.lock().unwrap().killed.push(s.name.to_string());
    }
    report.into_inner().unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Instant;

    const S: fn(u64) -> Duration = Duration::from_secs;

    struct Fake {
        /// Через сколько после SIGTERM процесс завершится сам (`None`: игнорирует).
        stops_after: Option<Duration>,
        termed: Mutex<Option<Instant>>,
        killed: Mutex<bool>,
        events: Events,
        name: &'static str,
    }

    impl Proc for Fake {
        async fn term(&self) {
            *self.termed.lock().unwrap() = Some(Instant::now());
            self.events.lock().unwrap().push((self.name, "term", Instant::now()));
        }
        async fn exited(&self) -> bool {
            if *self.killed.lock().unwrap() {
                return true;
            }
            let termed = *self.termed.lock().unwrap();
            let done = matches!((termed, self.stops_after), (Some(at), Some(d)) if at.elapsed() >= d);
            if done {
                let mut events = self.events.lock().unwrap();
                if !events.iter().any(|(n, e, _)| *n == self.name && *e == "exit") {
                    events.push((self.name, "exit", Instant::now()));
                }
            }
            done
        }
        async fn kill(&self) {
            *self.killed.lock().unwrap() = true;
            self.events.lock().unwrap().push((self.name, "kill", Instant::now()));
        }
    }

    type Events = std::sync::Arc<Mutex<Vec<(&'static str, &'static str, Instant)>>>;

    fn world(spec: &[(&'static str, Option<u64>)]) -> (HashMap<&'static str, Fake>, Events) {
        let events: Events = Default::default();
        let procs = spec
            .iter()
            .map(|(name, d)| (*name, Fake { stops_after: d.map(S), termed: Mutex::new(None), killed: Mutex::new(false), events: events.clone(), name }))
            .collect();
        (procs, events)
    }

    fn svc(name: &'static str, deps: &[&'static str]) -> Service {
        Service { name, deps: deps.to_vec() }
    }

    fn graph() -> Vec<Service> {
        vec![svc("db", &[]), svc("api", &["db"]), svc("web", &["api"]), svc("worker", &["db"])]
    }

    fn at(events: &Events, name: &str, what: &str, t0: Instant) -> Duration {
        events.lock().unwrap().iter().find(|(n, e, _)| *n == name && *e == what).map(|(_, _, t)| t.duration_since(t0)).unwrap_or_else(|| panic!("нет события {what} у {name}"))
    }

    #[tokio::test(start_paused = true)]
    async fn dependents_stop_first_and_independent_ones_together() {
        let (procs, events) = world(&[("db", Some(1)), ("api", Some(1)), ("web", Some(1)), ("worker", Some(2))]);
        let t0 = Instant::now();
        let report = down(&graph(), &procs, S(5)).await;
        assert!(report.killed.is_empty());
        let pos = |n: &str| report.order.iter().position(|x| x == n).unwrap();
        assert!(pos("web") < pos("api") && pos("api") < pos("db") && pos("worker") < pos("db"), "порядок остановки нарушает зависимости: {:?}", report.order);
        assert!(at(&events, "web", "term", t0) < Duration::from_millis(100) && at(&events, "worker", "term", t0) < Duration::from_millis(100), "web и worker никому не нужны, их останавливают сразу и вместе");
        assert!(at(&events, "api", "term", t0) >= at(&events, "web", "exit", t0), "api получил SIGTERM раньше, чем web закончил работу");
        assert!(at(&events, "db", "term", t0) >= at(&events, "worker", "exit", t0) && at(&events, "db", "term", t0) >= at(&events, "api", "exit", t0), "базу остановили, пока на неё опирались");
        assert!(t0.elapsed() < S(3) + Duration::from_millis(400), "остановка заняла {:?}: независимые ветки остановлены по очереди", t0.elapsed());
    }

    #[tokio::test(start_paused = true)]
    async fn a_stuck_service_is_killed_after_the_grace_period() {
        let (procs, events) = world(&[("db", Some(1)), ("api", None), ("web", Some(1)), ("worker", Some(1))]);
        let t0 = Instant::now();
        let report = down(&graph(), &procs, S(5)).await;
        assert_eq!(report.killed, vec!["api".to_string()]);
        let killed_at = at(&events, "api", "kill", t0);
        assert!(killed_at >= S(1) + S(5) && killed_at < S(1) + S(5) + Duration::from_millis(200), "api добит в {killed_at:?}, а должен через 5 с после SIGTERM (который пошёл в 1 с)");
        assert!(at(&events, "db", "term", t0) >= killed_at, "база ждёт, пока api не исчез совсем, пусть и принудительно");
    }

    #[tokio::test(start_paused = true)]
    async fn a_polite_service_is_never_killed() {
        let (procs, _) = world(&[("db", Some(4)), ("api", Some(4)), ("web", Some(4)), ("worker", Some(4))]);
        let report = down(&graph(), &procs, S(5)).await;
        assert!(report.killed.is_empty(), "добиты вежливые: {:?}", report.killed);
        assert_eq!(report.order.len(), 4);
    }
}
