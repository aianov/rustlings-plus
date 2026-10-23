use devctl::{Backend, Config, ServiceSpec, Stack, StackError};
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::time::Duration;
use tokio::time::Instant;

const S: fn(u64) -> Duration = Duration::from_secs;
const MS: fn(u64) -> Duration = Duration::from_millis;

#[derive(Clone)]
struct Plan {
    warmup: Option<Duration>,
    stop_after: Option<Duration>,
}

#[derive(Default)]
struct Proc {
    running: bool,
    started_at: Option<Instant>,
    termed_at: Option<Instant>,
    in_flight: usize,
    starts: u32,
}

struct Fake {
    plans: HashMap<String, Plan>,
    procs: Mutex<HashMap<String, Proc>>,
    events: Mutex<Vec<(String, &'static str, Instant)>>,
}

impl Fake {
    fn new(default: Plan, overrides: &[(&str, Plan)]) -> Fake {
        let _ = &default;
        let mut plans = HashMap::new();
        for name in ["nats", "auth", "user", "gateway", "post-service-1", "post-service-2", "post-service-3", "post-service", "chat"] {
            plans.insert(name.to_string(), default.clone());
        }
        for (name, plan) in overrides {
            plans.insert(name.to_string(), plan.clone());
        }
        Fake { plans, procs: Mutex::new(HashMap::new()), events: Mutex::new(Vec::new()) }
    }

    fn log(&self, instance: &str, what: &'static str) {
        self.events.lock().unwrap().push((instance.to_string(), what, Instant::now()));
    }

    fn when(&self, instance: &str, what: &str, t0: Instant) -> Duration {
        self.events.lock().unwrap().iter().find(|(i, w, _)| i == instance && *w == what).map(|(_, _, t)| t.duration_since(t0)).unwrap_or_else(|| panic!("нет события {what} у {instance}"))
    }

    fn count(&self, instance: &str, what: &str) -> usize {
        self.events.lock().unwrap().iter().filter(|(i, w, _)| i == instance && *w == what).count()
    }

    fn running(&self) -> Vec<String> {
        let mut v: Vec<String> = self.procs.lock().unwrap().iter().filter(|(_, p)| p.running).map(|(n, _)| n.clone()).collect();
        v.sort();
        v
    }

    /// Запрос пользователя к экземпляру: 50 мс. `false`, если процесс остановили посреди запроса.
    async fn request(&self, instance: &str) -> bool {
        self.procs.lock().unwrap().get_mut(instance).unwrap().in_flight += 1;
        tokio::time::sleep(MS(50)).await;
        let mut procs = self.procs.lock().unwrap();
        let p = procs.get_mut(instance).unwrap();
        p.in_flight -= 1;
        p.running
    }
}

impl Backend for Fake {
    async fn start(&self, instance: &str) {
        self.log(instance, "start");
        let mut procs = self.procs.lock().unwrap();
        let p = procs.entry(instance.to_string()).or_default();
        p.running = true;
        p.started_at = Some(Instant::now());
        p.termed_at = None;
        p.starts += 1;
    }
    async fn ready(&self, instance: &str) -> bool {
        let procs = self.procs.lock().unwrap();
        let Some(p) = procs.get(instance).filter(|p| p.running) else { return false };
        self.plans[instance].warmup.is_some_and(|w| p.started_at.unwrap().elapsed() >= w)
    }
    async fn term(&self, instance: &str) {
        self.log(instance, "term");
        if let Some(p) = self.procs.lock().unwrap().get_mut(instance) {
            p.termed_at = Some(Instant::now());
        }
    }
    async fn exited(&self, instance: &str) -> bool {
        let mut procs = self.procs.lock().unwrap();
        let Some(p) = procs.get_mut(instance) else { return true };
        if !p.running {
            return true;
        }
        if let (Some(at), Some(after)) = (p.termed_at, self.plans[instance].stop_after) {
            if at.elapsed() >= after {
                p.running = false;
                drop(procs);
                self.log(instance, "exit");
                return true;
            }
        }
        false
    }
    async fn kill(&self, instance: &str) {
        self.log(instance, "kill");
        if let Some(p) = self.procs.lock().unwrap().get_mut(instance) {
            p.running = false;
        }
    }
    async fn in_flight(&self, instance: &str) -> usize {
        self.procs.lock().unwrap().get(instance).map_or(0, |p| p.in_flight)
    }
}

fn spec(name: &'static str, deps: &[&'static str], replicas: usize) -> ServiceSpec {
    ServiceSpec { name, deps: deps.to_vec(), replicas }
}

fn social() -> Vec<ServiceSpec> {
    vec![spec("gateway", &["auth", "user", "post-service"], 1), spec("post-service", &["nats"], 3), spec("auth", &["nats"], 1), spec("user", &[], 1), spec("nats", &[], 1)]
}

fn plan(warmup: u64, stop: u64) -> Plan {
    Plan { warmup: Some(S(warmup)), stop_after: Some(S(stop)) }
}

#[tokio::test(start_paused = true)]
async fn up_starts_in_dependency_order_and_in_parallel_where_possible() {
    let fake = Fake::new(plan(1, 1), &[("nats", plan(3, 1)), ("user", plan(5, 1))]);
    let stack = Stack::new(fake, social(), Config::default());
    let t0 = Instant::now();
    stack.up().await.unwrap();
    let f = stack.backend();
    assert!(f.when("nats", "start", t0) < MS(50) && f.when("user", "start", t0) < MS(50), "nats и user ни от кого не зависят и стартуют сразу");
    for dep_start in ["auth", "post-service-1", "post-service-2", "post-service-3"] {
        let at = f.when(dep_start, "start", t0);
        assert!(at >= S(3) && at < S(3) + MS(250), "{dep_start} стартовал в {at:?}: ждать нужно готовности NATS (3 с)");
    }
    let gateway = f.when("gateway", "start", t0);
    assert!(gateway >= S(5) && gateway < S(5) + MS(350), "gateway ждёт и user (готов в 5 с), и post-service (готов в 4 с): старт в {gateway:?}");
    assert!(t0.elapsed() < S(6) + MS(500), "всё поднялось за {:?}", t0.elapsed());
    for (service, expected) in [("nats", 1), ("auth", 1), ("user", 1), ("gateway", 1), ("post-service", 3)] {
        assert_eq!(stack.serving(service).len(), expected, "в круге {service} должны быть все готовые экземпляры");
    }
}

#[tokio::test(start_paused = true)]
async fn graph_errors_are_reported_before_anything_starts() {
    let fake = Fake::new(plan(1, 1), &[]);
    let stack = Stack::new(fake, vec![spec("auth", &["nats"], 1), spec("nats", &["auth"], 1)], Config::default());
    assert!(matches!(stack.up().await, Err(StackError::Cycle(_))));
    let stack2 = Stack::new(Fake::new(plan(1, 1), &[]), vec![spec("auth", &["ghost"], 1)], Config::default());
    assert_eq!(stack2.up().await, Err(StackError::Unknown("ghost".into())));
    assert!(stack.backend().running().is_empty() && stack2.backend().running().is_empty());
}

#[tokio::test(start_paused = true)]
async fn a_failed_start_rolls_everything_back() {
    let never = Plan { warmup: None, stop_after: Some(S(1)) };
    let fake = Fake::new(plan(1, 1), &[("post-service-2", never)]);
    let cfg = Config { ready_timeout: S(10), ..Config::default() };
    let stack = Stack::new(fake, social(), cfg);
    let err = stack.up().await.unwrap_err();
    assert_eq!(err, StackError::NotReady("post-service-2".into()));
    assert!(stack.backend().running().is_empty(), "после неудачного запуска остались живые процессы: {:?}", stack.backend().running());
    assert!(stack.backend().count("gateway", "start") == 0, "gateway не должен был стартовать");
    assert!(stack.serving("post-service").is_empty() && stack.serving("nats").is_empty(), "круг должен быть пуст");
}

#[tokio::test(start_paused = true)]
async fn down_goes_in_reverse_order_and_kills_the_stuck() {
    let stuck = Plan { warmup: Some(S(1)), stop_after: None };
    let fake = Fake::new(plan(1, 1), &[("auth", stuck)]);
    let stack = Stack::new(fake, social(), Config::default());
    stack.up().await.unwrap();
    let t0 = Instant::now();
    stack.down().await;
    let f = stack.backend();
    assert!(f.running().is_empty(), "после остановки живы: {:?}", f.running());
    assert!(f.when("gateway", "term", t0) < MS(100), "gateway никому не нужен, его останавливают первым");
    for inst in ["auth", "post-service-1", "post-service-2", "post-service-3"] {
        assert!(f.when(inst, "term", t0) >= f.when("gateway", "exit", t0), "{inst} получил SIGTERM раньше, чем остановился gateway");
        assert!(f.when("nats", "term", t0) >= f.when(inst, if inst == "auth" { "kill" } else { "exit" }, t0), "NATS остановили, пока на него опирался {inst}");
    }
    let killed = f.when("auth", "kill", t0);
    assert!(killed >= S(1) + S(5) && killed < S(1) + S(5) + MS(200), "зависший auth добит в {killed:?}: через 5 с после SIGTERM");
    assert_eq!(f.count("post-service-1", "kill"), 0, "вежливых не добивают");
    assert!(stack.serving("post-service").is_empty());
}

#[tokio::test(start_paused = true)]
async fn restart_is_invisible_to_users() {
    let fake = Fake::new(plan(1, 1), &[]);
    let stack = std::sync::Arc::new(Stack::new(fake, social(), Config::default()));
    stack.up().await.unwrap();
    let failed = std::sync::Arc::new(AtomicU32::new(0));
    let ok = std::sync::Arc::new(AtomicU32::new(0));
    let min_serving = std::sync::Arc::new(AtomicUsize::new(3));
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let load = {
        let (stack, failed, ok, stop) = (stack.clone(), failed.clone(), ok.clone(), stop.clone());
        tokio::spawn(async move {
            let mut tasks = Vec::new();
            let mut n = 0usize;
            while !stop.load(Ordering::SeqCst) {
                let serving = stack.serving("post-service");
                n += 1;
                if serving.is_empty() {
                    failed.fetch_add(1, Ordering::SeqCst);
                } else {
                    let instance = serving[n % serving.len()].clone();
                    let (stack, failed, ok) = (stack.clone(), failed.clone(), ok.clone());
                    tasks.push(tokio::spawn(async move {
                        if stack.backend().request(&instance).await { ok.fetch_add(1, Ordering::SeqCst) } else { failed.fetch_add(1, Ordering::SeqCst) };
                    }));
                }
                tokio::time::sleep(MS(10)).await;
            }
            for t in tasks {
                let _ = t.await;
            }
        })
    };
    let watcher = {
        let (stack, min, stop) = (stack.clone(), min_serving.clone(), stop.clone());
        tokio::spawn(async move {
            while !stop.load(Ordering::SeqCst) {
                min.fetch_min(stack.serving("post-service").len(), Ordering::SeqCst);
                tokio::time::sleep(MS(5)).await;
            }
        })
    };
    tokio::time::sleep(MS(500)).await;
    let t0 = Instant::now();
    stack.restart("post-service").await.unwrap();
    tokio::time::sleep(MS(500)).await;
    stop.store(true, Ordering::SeqCst);
    load.await.unwrap();
    watcher.await.unwrap();
    let f = stack.backend();
    assert_eq!(failed.load(Ordering::SeqCst), 0, "во время выката ошибок у пользователей: {}", failed.load(Ordering::SeqCst));
    assert!(ok.load(Ordering::SeqCst) > 150);
    assert!(min_serving.load(Ordering::SeqCst) >= 2, "в круге было всего {} экземпляров из трёх", min_serving.load(Ordering::SeqCst));
    for i in 1..=3 {
        assert_eq!(f.procs.lock().unwrap()[&format!("post-service-{i}")].starts, 2, "экземпляр {i} должен быть перезапущен ровно один раз");
    }
    assert_eq!(f.count("nats", "term") + f.count("auth", "term") + f.count("gateway", "term"), 0, "остальные сервисы не трогаются");
    assert_eq!(stack.serving("post-service").len(), 3);
    assert!(t0.elapsed() < S(8));
}

#[tokio::test(start_paused = true)]
async fn restart_edge_cases() {
    let stack = Stack::new(Fake::new(plan(1, 1), &[]), social(), Config::default());
    stack.up().await.unwrap();
    assert_eq!(stack.restart("ghost").await, Err(StackError::Unknown("ghost".into())));
    let before = stack.backend().count("auth", "start");
    stack.restart("auth").await.unwrap();
    assert_eq!(stack.backend().count("auth", "start"), before + 1, "единственный экземпляр перезапущен");
    assert_eq!(stack.serving("auth"), vec!["auth".to_string()], "и вернулся в круг");
    let slow_start = Stack::new(Fake::new(plan(1, 1), &[("user", Plan { warmup: Some(S(3)), stop_after: Some(S(1)) })]), social(), Config::default());
    slow_start.up().await.unwrap();
    let p = std::sync::Arc::new(slow_start);
    let task = {
        let p = p.clone();
        tokio::spawn(async move { p.restart("user").await })
    };
    tokio::time::sleep(S(2)).await;
    assert!(p.serving("user").is_empty(), "перезапускаемый экземпляр ещё не готов и не должен получать запросы");
    task.await.unwrap().unwrap();
    assert_eq!(p.serving("user").len(), 1);
}
