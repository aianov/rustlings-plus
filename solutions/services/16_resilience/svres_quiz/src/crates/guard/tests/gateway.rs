use guard::backend::Backend;
use guard::deadline::Deadline;
use guard::retry::{Code, Status};
use guard::{CallError, Config, Gateway, Request, Response};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::Instant;

const MS: fn(u64) -> Duration = Duration::from_millis;

#[derive(Clone, Copy)]
enum Step {
    Ok,
    Fail(Code),
    /// Отвечает ошибкой, но не сразу.
    SlowFail(Code, Duration),
    Hang(Duration),
}

/// Подмена зависимостей: по зависимости очередь шагов, когда она кончилась, берётся шаг по умолчанию.
struct Script {
    queues: Mutex<HashMap<&'static str, VecDeque<Step>>>,
    default: Mutex<HashMap<&'static str, Step>>,
    calls: AtomicU32,
    log: Mutex<Vec<(&'static str, Instant, Duration)>>,
}

impl Script {
    fn new() -> Arc<Script> {
        Arc::new(Script { queues: Mutex::new(HashMap::new()), default: Mutex::new(HashMap::new()), calls: AtomicU32::new(0), log: Mutex::new(Vec::new()) })
    }
    fn queue(&self, dep: &'static str, steps: &[Step]) {
        self.queues.lock().unwrap().entry(dep).or_default().extend(steps.iter().copied());
    }
    fn always(&self, dep: &'static str, step: Step) {
        self.default.lock().unwrap().insert(dep, step);
    }
    fn calls(&self) -> u32 {
        self.calls.load(Ordering::SeqCst)
    }
}

impl Backend for Script {
    async fn call(&self, dep: &'static str, _req: Request, deadline: Deadline) -> Result<Response, Status> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.log.lock().unwrap().push((dep, Instant::now(), deadline.remaining()));
        let step = {
            let queued = self.queues.lock().unwrap().get_mut(dep).and_then(|q| q.pop_front());
            queued.or_else(|| self.default.lock().unwrap().get(dep).copied()).unwrap_or(Step::Ok)
        };
        match step {
            Step::Ok => Ok(Response(format!("{dep}: ok"))),
            Step::Fail(code) => Err(Status(code)),
            Step::SlowFail(code, after) => {
                tokio::time::sleep(after).await;
                Err(Status(code))
            }
            Step::Hang(how_long) => {
                tokio::time::sleep(how_long).await;
                Ok(Response(format!("{dep}: late")))
            }
        }
    }
}

fn request() -> Request {
    Request { body: "x".into(), idempotency_key: None }
}

fn gateway(script: &Arc<Script>) -> Gateway<Arc<Script>> {
    Gateway::new(script.clone(), Config::default())
}

#[tokio::test(start_paused = true)]
async fn a_transient_failure_is_retried() {
    let script = Script::new();
    script.queue("user", &[Step::Fail(Code::Unavailable), Step::Fail(Code::Unavailable), Step::Ok]);
    let gw = gateway(&script);
    let r = gw.call("user", request(), true, Duration::from_secs(5)).await;
    assert_eq!(r, Ok(Response("user: ok".into())), "две временные ошибки подряд и успех: запрос должен пережить");
    assert_eq!(script.calls(), 3);
}

#[tokio::test(start_paused = true)]
async fn caller_errors_are_not_retried() {
    let script = Script::new();
    script.always("user", Step::Fail(Code::InvalidArgument));
    let gw = gateway(&script);
    let r = gw.call("user", request(), true, Duration::from_secs(5)).await;
    assert_eq!(r, Err(CallError::Failed(Status(Code::InvalidArgument))));
    assert_eq!(script.calls(), 1, "InvalidArgument повторять бессмысленно");
}

#[tokio::test(start_paused = true)]
async fn non_idempotent_requests_are_not_retried_after_ambiguous_errors() {
    let script = Script::new();
    script.always("user", Step::Fail(Code::Internal));
    let gw = gateway(&script);
    let r = gw.call("user", request(), false, Duration::from_secs(5)).await;
    assert_eq!(r, Err(CallError::Failed(Status(Code::Internal))));
    assert_eq!(script.calls(), 1, "неидемпотентный запрос после Internal мог выполниться, повторять нельзя");
}

#[tokio::test(start_paused = true)]
async fn the_deadline_cuts_a_hanging_call_exactly() {
    let script = Script::new();
    script.always("user", Step::Hang(Duration::from_secs(60)));
    let gw = gateway(&script);
    let start = Instant::now();
    let r = gw.call("user", request(), true, MS(500)).await;
    assert_eq!(r, Err(CallError::DeadlineExceeded));
    assert_eq!(start.elapsed(), MS(500), "клиент должен получить отказ ровно в срок, а не после таймаутов всех попыток");
    assert_eq!(script.calls(), 1, "после срока новых попыток быть не должно");
}

#[tokio::test(start_paused = true)]
async fn the_remaining_budget_travels_down_and_shrinks() {
    let script = Script::new();
    script.always("user", Step::SlowFail(Code::Unavailable, MS(100)));
    let gw = gateway(&script);
    let _ = gw.call("user", request(), true, Duration::from_secs(2)).await;
    let seen: Vec<Duration> = script.log.lock().unwrap().iter().map(|(_, _, left)| *left).collect();
    assert!(seen.len() >= 2);
    assert_eq!(seen[0], Duration::from_secs(2), "первая попытка получает весь бюджет");
    assert!(seen[1] <= Duration::from_secs(2) - MS(100), "вторая попытка получает остаток, а не бюджет заново: {seen:?}");
}

#[tokio::test(start_paused = true)]
async fn retries_never_run_past_the_deadline() {
    let script = Script::new();
    script.always("user", Step::Fail(Code::Unavailable));
    let mut cfg = Config { attempts: 10, base: MS(100), retry_reserve: 100, ..Config::default() };
    cfg.breaker.min_calls = 1000;
    cfg.breaker.window = 1000;
    let gw = Gateway::new(script.clone(), cfg);
    let start = Instant::now();
    let r = gw.call("user", request(), true, MS(300)).await;
    assert_eq!(r, Err(CallError::Failed(Status(Code::Unavailable))));
    assert!(start.elapsed() <= MS(300), "запрос с бюджетом 300 мс шёл {:?}", start.elapsed());
    assert!(script.calls() >= 2 && script.calls() <= 10, "попыток {}", script.calls());
    let last_start = script.log.lock().unwrap().last().unwrap().1;
    assert!(last_start - start < MS(300), "последняя попытка началась уже после срока");
}

#[tokio::test(start_paused = true)]
async fn the_breaker_stops_calls_to_a_dead_dependency_and_heals() {
    let script = Script::new();
    script.always("user", Step::Fail(Code::Unavailable));
    let gw = gateway(&script);
    for _ in 0..3 {
        let _ = gw.call("user", request(), true, Duration::from_secs(5)).await;
    }
    let before = script.calls();
    for _ in 0..20 {
        let r = gw.call("user", request(), true, Duration::from_secs(5)).await;
        assert!(matches!(r, Err(CallError::CircuitOpen { .. })), "предохранитель должен быть открыт, а ответ {r:?}");
    }
    assert_eq!(script.calls(), before, "открытый предохранитель не должен пропускать вызовы в зависимость");
    assert_eq!(gw.call("post", request(), true, Duration::from_secs(5)).await, Ok(Response("post: ok".into())), "предохранитель одной зависимости не трогает другую");
    script.always("user", Step::Ok);
    tokio::time::advance(Duration::from_secs(6)).await;
    assert_eq!(gw.call("user", request(), true, Duration::from_secs(5)).await, Ok(Response("user: ok".into())), "после паузы проба проходит и закрывает предохранитель");
    assert!(gw.call("user", request(), true, Duration::from_secs(5)).await.is_ok());
}

#[tokio::test(start_paused = true)]
async fn caller_mistakes_do_not_trip_the_breaker() {
    let script = Script::new();
    script.always("user", Step::Fail(Code::NotFound));
    let gw = gateway(&script);
    for _ in 0..30 {
        assert_eq!(gw.call("user", request(), true, Duration::from_secs(5)).await, Err(CallError::Failed(Status(Code::NotFound))));
    }
    script.always("user", Step::Ok);
    assert!(gw.call("user", request(), true, Duration::from_secs(5)).await.is_ok(), "зависимость здорова, это клиент просил несуществующее");
}

#[tokio::test(start_paused = true)]
async fn a_slow_dependency_cannot_take_places_of_another() {
    let script = Script::new();
    script.always("user", Step::Hang(Duration::from_secs(3600)));
    let gw = Arc::new(gateway(&script));
    let mut stuck = Vec::new();
    for _ in 0..4 {
        let gw = gw.clone();
        stuck.push(tokio::spawn(async move { gw.call("user", request(), true, Duration::from_secs(3600)).await }));
    }
    tokio::time::sleep(MS(1)).await;
    let start = Instant::now();
    assert_eq!(gw.call("user", request(), true, Duration::from_secs(5)).await, Err(CallError::Full), "пятый вызов к user при лимите четыре");
    assert_eq!(gw.call("post", request(), true, Duration::from_secs(5)).await, Ok(Response("post: ok".into())), "post-service не должен страдать от user-service");
    assert_eq!(start.elapsed(), Duration::ZERO);
    assert_eq!(gw.call("billing", request(), true, Duration::from_secs(5)).await, Err(CallError::UnknownDep));
}

#[tokio::test(start_paused = true)]
async fn the_gateway_sheds_load_at_the_entrance() {
    let script = Script::new();
    script.always("user", Step::Hang(Duration::from_secs(3600)));
    script.always("post", Step::Hang(Duration::from_secs(3600)));
    let cfg = Config { max_in_flight: 3, ..Config::default() };
    let gw = Arc::new(Gateway::new(script.clone(), cfg));
    let mut stuck = Vec::new();
    for i in 0..3 {
        let gw = gw.clone();
        let dep = if i == 0 { "user" } else { "post" };
        stuck.push(tokio::spawn(async move { gw.call(dep, request(), true, MS(10_000)).await }));
    }
    tokio::time::sleep(MS(1)).await;
    let start = Instant::now();
    assert_eq!(gw.call("post", request(), true, Duration::from_secs(5)).await, Err(CallError::Overloaded), "четвёртый запрос при лимите три");
    assert_eq!(start.elapsed(), Duration::ZERO, "отказ у входа мгновенный");
    tokio::time::advance(MS(11_000)).await;
    for t in stuck {
        assert_eq!(t.await.unwrap(), Err(CallError::DeadlineExceeded));
    }
    script.always("post", Step::Ok);
    assert!(gw.call("post", request(), true, Duration::from_secs(5)).await.is_ok(), "места вернулись");
}

#[tokio::test(start_paused = true)]
async fn retries_are_limited_by_the_budget_during_an_outage() {
    let script = Script::new();
    script.always("user", Step::Fail(Code::Unavailable));
    let mut cfg = Config::default();
    cfg.breaker.min_calls = 100_000;
    cfg.breaker.window = 100_000;
    let gw = Gateway::new(script.clone(), cfg);
    for _ in 0..200 {
        let _ = gw.call("user", request(), true, Duration::from_secs(30)).await;
    }
    let calls = script.calls();
    assert!(calls >= 200, "каждый запрос обязан попробовать хотя бы раз, а обращений {calls}");
    assert!(calls <= 235, "200 запросов к лежащему сервису породили {calls} обращений: повторы не ограничены десятой долей плюс резерв");
}

/// Подмена post-service, которая уважает ключи идемпотентности и умеет потерять ответ.
struct Posts {
    created: Mutex<HashMap<String, u64>>,
    lose_once: Mutex<bool>,
}

impl Backend for Posts {
    async fn call(&self, _dep: &'static str, req: Request, _deadline: Deadline) -> Result<Response, Status> {
        let id = {
            let mut created = self.created.lock().unwrap();
            let next = created.len() as u64 + 1;
            match &req.idempotency_key {
                Some(key) => *created.entry(key.clone()).or_insert(next),
                None => {
                    created.insert(format!("anon-{next}"), next);
                    next
                }
            }
        };
        let lose = std::mem::replace(&mut *self.lose_once.lock().unwrap(), false);
        if lose {
            return Err(Status(Code::Unavailable));
        }
        Ok(Response(format!("post {id}")))
    }
}

#[tokio::test(start_paused = true)]
async fn a_lost_response_does_not_make_a_second_post() {
    let posts = Arc::new(Posts { created: Mutex::new(HashMap::new()), lose_once: Mutex::new(true) });
    let gw = Gateway::new(posts.clone(), Config::default());
    let r = gw.create_post(1, "привет", Duration::from_secs(5)).await;
    assert_eq!(r, Ok(Response("post 1".into())), "пост создан, ответ потерялся, повтор с тем же ключом должен вернуть тот же пост");
    assert_eq!(posts.created.lock().unwrap().len(), 1, "в базе должен быть ровно один пост");
}

#[tokio::test(start_paused = true)]
async fn different_requests_get_different_keys() {
    let posts = Arc::new(Posts { created: Mutex::new(HashMap::new()), lose_once: Mutex::new(false) });
    let gw = Gateway::new(posts.clone(), Config::default());
    gw.create_post(1, "один", Duration::from_secs(5)).await.unwrap();
    gw.create_post(1, "два", Duration::from_secs(5)).await.unwrap();
    gw.create_post(2, "три", Duration::from_secs(5)).await.unwrap();
    assert_eq!(posts.created.lock().unwrap().len(), 3, "разные запросы не должны сливаться в один общим ключом");
}
