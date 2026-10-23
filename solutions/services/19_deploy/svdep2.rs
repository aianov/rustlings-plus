
use futures::future::try_join_all;
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use tokio::sync::watch;

fn main() {}

#[derive(Debug, Clone)]
pub struct Service {
    pub name: &'static str,
    pub deps: Vec<&'static str>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum StartError {
    /// Цикл зависимостей: имена по кругу.
    Cycle(Vec<String>),
    Unknown(String),
    NotReady(String),
}

#[allow(async_fn_in_trait)]
pub trait Runner {
    async fn start(&self, name: &str);
    async fn ready(&self, name: &str) -> bool;
}

/// Запускает всё. Возвращает имена в порядке запуска.
pub async fn up<R: Runner>(services: &[Service], runner: &R, ready_timeout: Duration) -> Result<Vec<String>, StartError> {
    let names: HashSet<&str> = services.iter().map(|s| s.name).collect();
    for s in services {
        if let Some(missing) = s.deps.iter().find(|d| !names.contains(*d)) {
            return Err(StartError::Unknown((*missing).to_string()));
        }
    }
    if let Some(cycle) = find_cycle(services) {
        return Err(StartError::Cycle(cycle));
    }
    let channels: HashMap<&str, (watch::Sender<bool>, watch::Receiver<bool>)> = services.iter().map(|s| (s.name, watch::channel(false))).collect();
    let order = std::sync::Mutex::new(Vec::new());
    try_join_all(services.iter().map(|s| {
        let channels = &channels;
        let order = &order;
        async move {
            for dep in &s.deps {
                let mut rx = channels[dep].1.clone();
                let _ = rx.wait_for(|ready| *ready).await;
            }
            runner.start(s.name).await;
            order.lock().unwrap().push(s.name.to_string());
            let started = tokio::time::Instant::now();
            while !runner.ready(s.name).await {
                if started.elapsed() >= ready_timeout {
                    return Err(StartError::NotReady(s.name.to_string()));
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            let _ = channels[s.name].0.send(true);
            Ok(())
        }
    }))
    .await?;
    Ok(order.into_inner().unwrap())
}

fn find_cycle(services: &[Service]) -> Option<Vec<String>> {
    fn visit<'a>(name: &'a str, by_name: &HashMap<&'a str, &'a Service>, path: &mut Vec<&'a str>, done: &mut HashSet<&'a str>) -> Option<Vec<String>> {
        if let Some(at) = path.iter().position(|p| *p == name) {
            return Some(path[at..].iter().map(|s| s.to_string()).collect());
        }
        if done.contains(name) {
            return None;
        }
        path.push(name);
        for dep in &by_name[name].deps {
            if let Some(found) = visit(dep, by_name, path, done) {
                return Some(found);
            }
        }
        path.pop();
        done.insert(name);
        None
    }
    let by_name: HashMap<&str, &Service> = services.iter().map(|s| (s.name, s)).collect();
    let mut done = HashSet::new();
    services.iter().find_map(|s| visit(s.name, &by_name, &mut Vec::new(), &mut done))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tokio::time::Instant;

    const S: fn(u64) -> Duration = Duration::from_secs;

    /// Сервис «готов» через `warmup` после запуска; `never` не становится готовым никогда.
    struct Fake {
        warmup: HashMap<&'static str, Option<Duration>>,
        started: Mutex<HashMap<String, Instant>>,
    }

    impl Fake {
        fn new(warmup: &[(&'static str, Option<u64>)]) -> Fake {
            Fake { warmup: warmup.iter().map(|(n, w)| (*n, w.map(S))).collect(), started: Mutex::new(HashMap::new()) }
        }
        fn started_at(&self, name: &str) -> Duration {
            let started = self.started.lock().unwrap();
            started[name].duration_since(*started.values().min().unwrap())
        }
    }

    impl Runner for Fake {
        async fn start(&self, name: &str) {
            self.started.lock().unwrap().insert(name.to_string(), Instant::now());
        }
        async fn ready(&self, name: &str) -> bool {
            let Some(at) = self.started.lock().unwrap().get(name).copied() else { return false };
            self.warmup[name].is_some_and(|w| at.elapsed() >= w)
        }
    }

    fn svc(name: &'static str, deps: &[&'static str]) -> Service {
        Service { name, deps: deps.to_vec() }
    }

    #[tokio::test(start_paused = true)]
    async fn a_service_starts_only_after_its_dependencies_are_ready() {
        let fake = Fake::new(&[("nats", Some(3)), ("auth", Some(1)), ("gateway", Some(1))]);
        let services = [svc("gateway", &["auth"]), svc("auth", &["nats"]), svc("nats", &[])];
        let start = Instant::now();
        let order = up(&services, &fake, S(30)).await.unwrap();
        assert_eq!(order, vec!["nats", "auth", "gateway"], "порядок запуска должен следовать зависимостям, а не списку");
        let (auth, gateway) = (fake.started_at("auth"), fake.started_at("gateway"));
        assert!(auth >= S(3) && auth < S(3) + Duration::from_millis(250), "auth стартовал в {auth:?}: ждать нужно готовности NATS (3 с), не больше");
        assert!(gateway >= S(4) && gateway < S(4) + Duration::from_millis(350), "gateway стартовал в {gateway:?}");
        assert!(start.elapsed() < S(5) + Duration::from_millis(500));
    }

    #[tokio::test(start_paused = true)]
    async fn independent_services_start_together() {
        let fake = Fake::new(&[("nats", Some(3)), ("auth", Some(1)), ("user", Some(4)), ("gateway", Some(2))]);
        let services = [svc("nats", &[]), svc("user", &[]), svc("auth", &["nats"]), svc("gateway", &["auth", "user"])];
        let start = Instant::now();
        up(&services, &fake, S(30)).await.unwrap();
        assert_eq!(fake.started_at("user"), Duration::ZERO, "user ни от кого не зависит, ждать ему нечего");
        assert!(fake.started_at("nats") < Duration::from_millis(50));
        let gateway = fake.started_at("gateway");
        assert!(gateway >= S(4) && gateway < S(4) + Duration::from_millis(350), "gateway ждёт и auth (готов в 4 с), и user (готов в 4 с): старт в {gateway:?}");
        assert!(start.elapsed() < S(6) + Duration::from_millis(500), "всё поднимается за {:?}: независимые сервисы запускались по очереди", start.elapsed());
    }

    #[tokio::test(start_paused = true)]
    async fn cycles_and_unknown_dependencies_fail_before_anything_starts() {
        let fake = Fake::new(&[("a", Some(1)), ("b", Some(1)), ("c", Some(1))]);
        let cyc = up(&[svc("a", &["b"]), svc("b", &["c"]), svc("c", &["a"])], &fake, S(30)).await.unwrap_err();
        match cyc {
            StartError::Cycle(path) => {
                let mut sorted = path.clone();
                sorted.sort();
                assert_eq!(sorted, vec!["a", "b", "c"], "в цикле участвуют все три: {path:?}");
            }
            other => panic!("ожидался цикл, получено {other:?}"),
        }
        assert_eq!(up(&[svc("a", &["ghost"])], &fake, S(30)).await.unwrap_err(), StartError::Unknown("ghost".into()));
        assert!(fake.started.lock().unwrap().is_empty(), "при ошибке графа ничего не должно быть запущено");
        assert!(matches!(up(&[svc("a", &["a"])], &fake, S(30)).await, Err(StartError::Cycle(_))), "сервис, зависящий от себя");
    }

    #[tokio::test(start_paused = true)]
    async fn a_service_that_never_gets_ready_is_an_error() {
        let fake = Fake::new(&[("nats", None), ("auth", Some(1))]);
        let start = Instant::now();
        let err = up(&[svc("nats", &[]), svc("auth", &["nats"])], &fake, S(10)).await.unwrap_err();
        assert_eq!(err, StartError::NotReady("nats".into()));
        assert!(start.elapsed() >= S(10) && start.elapsed() < S(11), "ждали {:?}", start.elapsed());
        assert!(!fake.started.lock().unwrap().contains_key("auth"), "зависимый сервис не должен был стартовать");
    }
}
