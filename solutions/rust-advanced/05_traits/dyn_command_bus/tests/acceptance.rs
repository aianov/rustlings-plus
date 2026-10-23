use command_bus::{Bus, Context, IntoResponse, Layer, Notifier, Response};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

fn resp(status: u16, body: &str) -> Response {
    Response { status, body: body.to_string() }
}

#[test]
fn into_response_conversions() {
    assert_eq!("hi".into_response(), resp(200, "hi"));
    assert_eq!(String::from("s").into_response(), resp(200, "s"));
    assert_eq!((201, "made".to_string()).into_response(), resp(201, "made"));
    assert_eq!(resp(418, "tea").into_response(), resp(418, "tea"));
}

#[test]
fn results_map_errors_to_500_unless_they_have_a_status() {
    let ok: Result<String, String> = Ok("fine".into());
    let err: Result<String, String> = Err("broken".into());
    let err404: Result<String, (u16, String)> = Err((404, "gone".into()));
    assert_eq!(ok.into_response(), resp(200, "fine"));
    assert_eq!(err.into_response(), resp(500, "broken"), "ошибка без своего статуса получает 500");
    assert_eq!(err404.into_response(), resp(404, "gone"), "собственный статус ошибки сохраняется");
}

#[test]
fn handlers_with_different_return_types_share_one_bus() {
    let mut bus = Bus::new();
    assert!(!bus.register("echo", |_, args| args.to_string()));
    assert!(!bus.register("ping", |_, _| "pong"));
    assert!(!bus.register("make", |_, args| (201, format!("made {args}"))));
    assert!(!bus.register("fail", |_, _| -> Result<String, String> { Err("nope".into()) }));
    let mut ctx = Context::new();
    assert_eq!(bus.dispatch("echo", &mut ctx, "a b"), resp(200, "a b"), "обработчик получает аргументы");
    assert_eq!(bus.dispatch("ping", &mut ctx, ""), resp(200, "pong"));
    assert_eq!(bus.dispatch("make", &mut ctx, "x"), resp(201, "made x"));
    assert_eq!(bus.dispatch("fail", &mut ctx, ""), resp(500, "nope"));
}

#[test]
fn unknown_command_and_replacement() {
    let mut bus = Bus::new();
    let mut ctx = Context::new();
    assert_eq!(bus.dispatch("ghost", &mut ctx, ""), resp(404, "unknown command: ghost"));
    bus.register("a", |_, _| "old");
    assert!(bus.register("a", |_, _| "new"), "повторная регистрация заменяет и сообщает об этом");
    assert_eq!(bus.dispatch("a", &mut ctx, "").body, "new");
}

#[test]
fn names_are_sorted() {
    let mut bus = Bus::new();
    for n in ["zeta", "alpha", "mid"] {
        bus.register(n, |_, _| "");
    }
    assert_eq!(bus.names(), vec!["alpha", "mid", "zeta"]);
}

#[derive(Debug, PartialEq)]
struct Counter(u32);

#[test]
fn context_carries_typed_state_between_commands() {
    let mut bus = Bus::new();
    bus.register("incr", |ctx, _| {
        let counter = ctx.get_mut::<Counter>().expect("счётчик должен быть в контексте");
        counter.0 += 1;
        counter.0.to_string()
    });
    bus.register("read", |ctx, _| match ctx.get::<Counter>() {
        Some(c) => format!("count={}", c.0),
        None => "no counter".to_string(),
    });
    let mut ctx = Context::new();
    assert_eq!(bus.dispatch("read", &mut ctx, "").body, "no counter");
    assert_eq!(ctx.insert(Counter(10)), None);
    bus.dispatch("incr", &mut ctx, "");
    bus.dispatch("incr", &mut ctx, "");
    assert_eq!(bus.dispatch("read", &mut ctx, "").body, "count=12");
    assert_eq!(ctx.insert(Counter(0)), Some(Counter(12)), "insert возвращает прежнее значение");
    ctx.insert(7u8);
    assert_eq!(ctx.get::<Counter>(), Some(&Counter(0)), "значения разных типов не мешают друг другу");
}

struct Recorder {
    lines: Mutex<Vec<String>>,
}

impl Notifier for Recorder {
    fn notify(&self, user: &str, text: &str) {
        self.lines.lock().expect("журнал отравлен").push(format!("{user}:{text}"));
    }
}

#[test]
fn notifier_lives_in_the_context_as_a_trait_object() {
    let recorder = Arc::new(Recorder { lines: Mutex::new(Vec::new()) });
    let mut ctx = Context::new();
    ctx.insert(Arc::clone(&recorder) as Arc<dyn Notifier>);
    let mut bus = Bus::new();
    bus.register("tell", |ctx, args| {
        let Some(notifier) = ctx.get::<Arc<dyn Notifier>>() else {
            return (503, "no notifier".to_string());
        };
        let (user, text) = args.split_once(' ').unwrap_or((args, ""));
        notifier.notify(user, text);
        (200, "sent".to_string())
    });
    assert_eq!(bus.dispatch("tell", &mut ctx, "ann hello there"), resp(200, "sent"));
    assert_eq!(*recorder.lines.lock().expect("журнал отравлен"), vec!["ann:hello there"]);
    let mut empty = Context::new();
    assert_eq!(bus.dispatch("tell", &mut empty, "ann x"), resp(503, "no notifier"));
}

#[test]
fn the_bus_is_shareable_between_threads() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Bus>();
    assert_send_sync::<Context>();
    let mut bus = Bus::new();
    bus.register("double", |_, args| args.parse::<u32>().map(|n| (n * 2).to_string()).map_err(|e| (400, e.to_string())));
    let bus = Arc::new(bus);
    let workers: Vec<_> = (1..=4u32)
        .map(|n| {
            let bus = Arc::clone(&bus);
            std::thread::spawn(move || {
                let mut ctx = Context::new();
                bus.dispatch("double", &mut ctx, &n.to_string())
            })
        })
        .collect();
    let got: Vec<Response> = workers.into_iter().map(|w| w.join().expect("поток упал")).collect();
    assert_eq!(got, vec![resp(200, "2"), resp(200, "4"), resp(200, "6"), resp(200, "8")]);
    let mut ctx = Context::new();
    assert_eq!(bus.dispatch("double", &mut ctx, "x").status, 400, "ошибка разбора с собственным статусом");
}

struct Gate {
    blocked: &'static str,
    seen: Arc<AtomicUsize>,
}

impl Layer for Gate {
    fn before(&self, command: &str, _args: &str) -> Option<Response> {
        self.seen.fetch_add(1, Ordering::SeqCst);
        (command == self.blocked).then(|| resp(403, "forbidden"))
    }
}

#[test]
fn a_layer_can_stop_a_command_before_its_handler_runs() {
    let seen = Arc::new(AtomicUsize::new(0));
    let mut bus = Bus::new();
    bus.register("admin", |ctx, _| {
        ctx.insert(Counter(1));
        "done"
    });
    bus.register("ping", |_, _| "pong");
    bus.layer(Gate { blocked: "admin", seen: Arc::clone(&seen) });
    let mut ctx = Context::new();
    assert_eq!(bus.dispatch("admin", &mut ctx, ""), resp(403, "forbidden"));
    assert_eq!(ctx.get::<Counter>(), None, "обработчик заблокированной команды не должен был запуститься");
    assert_eq!(bus.dispatch("ping", &mut ctx, ""), resp(200, "pong"), "остальные команды проходят");
    assert_eq!(seen.load(Ordering::SeqCst), 2, "слой видит каждую команду");
}

#[test]
fn layers_run_in_order_and_the_first_refusal_wins() {
    let first = Arc::new(AtomicUsize::new(0));
    let second = Arc::new(AtomicUsize::new(0));
    let mut bus = Bus::new();
    bus.register("x", |_, _| "ok");
    bus.layer(Gate { blocked: "x", seen: Arc::clone(&first) });
    bus.layer(Gate { blocked: "never", seen: Arc::clone(&second) });
    let mut ctx = Context::new();
    assert_eq!(bus.dispatch("x", &mut ctx, "").status, 403);
    assert_eq!(second.load(Ordering::SeqCst), 0, "после отказа первого слоя остальные не опрашиваются");
    bus.dispatch("y", &mut ctx, "");
    assert_eq!(second.load(Ordering::SeqCst), 1, "слои работают и до поиска обработчика: неизвестную команду они тоже видят");
}
