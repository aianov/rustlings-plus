// Три запроса идут одновременно, и их логи перемешаны: «загружаю профиль», «собираю ленту», «загружаю профиль»... Чей это
// профиль? Нужен контекст запроса, который сам прилипает ко всем событиям внутри. Цель: каждое событие, написанное при
// обработке запроса (в том числе в готовых помощниках ниже), оказывается внутри span `request` с полями `request_id`,
// `method`, `path`. Когда запрос обработан, в этом же span заполнено поле `status` (код ответа). Запросы живут
// одновременно в одном потоке, и контекст одного не должен просочиться в другой.


use tokio::task::yield_now;
use tracing::{info, warn};

fn main() {}

pub struct Request {
    pub id: u64,
    pub method: &'static str,
    pub path: String,
    pub user: u64,
}

async fn load_profile(user: u64) {
    yield_now().await;
    info!(user, "профиль загружен");
}

async fn load_feed(user: u64) -> usize {
    yield_now().await;
    info!(user, "лента собрана");
    yield_now().await;
    3
}

async fn serve(req: &Request) -> u16 {
    load_profile(req.user).await;
    if req.path.starts_with("/feed") {
        let n = load_feed(req.user).await;
        info!(user = req.user, posts = n, "ответ готов");
        200
    } else {
        warn!(user = req.user, "такого пути нет");
        404
    }
}

pub async fn handle(req: Request) -> u16 {
    let _ = &req;
    todo!()
}


#[cfg(test)]
#[allow(dead_code)]
mod capture {
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};
    use tracing::field::{Field, Visit};
    use tracing::span::{Attributes, Id, Record};
    use tracing::{Event, Level, Subscriber};
    use tracing_subscriber::Layer;
    use tracing_subscriber::layer::{Context, SubscriberExt};
    use tracing_subscriber::registry::LookupSpan;

    #[derive(Debug, Clone, PartialEq)]
    pub enum Val {
        Int(i128),
        Float(f64),
        Bool(bool),
        Text(String),
    }

    pub type Fields = BTreeMap<String, Val>;

    #[derive(Default)]
    struct Collect(Fields);

    impl Visit for Collect {
        fn record_i64(&mut self, f: &Field, v: i64) {
            self.0.insert(f.name().to_string(), Val::Int(v as i128));
        }
        fn record_u64(&mut self, f: &Field, v: u64) {
            self.0.insert(f.name().to_string(), Val::Int(v as i128));
        }
        fn record_f64(&mut self, f: &Field, v: f64) {
            self.0.insert(f.name().to_string(), Val::Float(v));
        }
        fn record_bool(&mut self, f: &Field, v: bool) {
            self.0.insert(f.name().to_string(), Val::Bool(v));
        }
        fn record_str(&mut self, f: &Field, v: &str) {
            self.0.insert(f.name().to_string(), Val::Text(v.to_string()));
        }
        fn record_debug(&mut self, f: &Field, v: &dyn std::fmt::Debug) {
            self.0.insert(f.name().to_string(), Val::Text(format!("{v:?}")));
        }
    }

    #[derive(Debug, Clone)]
    pub struct SpanRec {
        pub name: String,
        pub fields: Fields,
    }

    impl SpanRec {
        pub fn int(&self, name: &str) -> Option<i128> {
            match self.fields.get(name) {
                Some(Val::Int(n)) => Some(*n),
                _ => None,
            }
        }
        pub fn text(&self, name: &str) -> Option<String> {
            match self.fields.get(name) {
                Some(Val::Text(s)) => Some(s.clone()),
                _ => None,
            }
        }
    }

    #[derive(Debug, Clone)]
    pub struct Rec {
        pub level: Level,
        pub target: String,
        pub fields: Fields,
        pub spans: Vec<SpanRec>,
    }

    impl Rec {
        pub fn message(&self) -> String {
            match self.fields.get("message") {
                Some(Val::Text(s)) => s.clone(),
                _ => String::new(),
            }
        }
        pub fn int(&self, name: &str) -> Option<i128> {
            match self.fields.get(name) {
                Some(Val::Int(n)) => Some(*n),
                _ => None,
            }
        }
        pub fn text(&self, name: &str) -> Option<String> {
            match self.fields.get(name) {
                Some(Val::Text(s)) => Some(s.clone()),
                _ => None,
            }
        }
        pub fn span(&self, name: &str) -> Option<&SpanRec> {
            self.spans.iter().find(|s| s.name == name)
        }
    }

    struct Stored(Fields);

    #[derive(Clone, Default)]
    pub struct Capture {
        events: Arc<Mutex<Vec<Rec>>>,
        closed: Arc<Mutex<Vec<SpanRec>>>,
    }

    impl Capture {
        pub fn events(&self) -> Vec<Rec> {
            self.events.lock().unwrap().clone()
        }
        pub fn closed(&self) -> Vec<SpanRec> {
            self.closed.lock().unwrap().clone()
        }
    }

    impl<S> Layer<S> for Capture
    where
        S: Subscriber + for<'a> LookupSpan<'a>,
    {
        fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
            let mut c = Collect::default();
            attrs.record(&mut c);
            if let Some(span) = ctx.span(id) {
                span.extensions_mut().insert(Stored(c.0));
            }
        }

        fn on_record(&self, id: &Id, values: &Record<'_>, ctx: Context<'_, S>) {
            if let Some(span) = ctx.span(id) {
                let mut ext = span.extensions_mut();
                if let Some(stored) = ext.get_mut::<Stored>() {
                    let mut c = Collect(std::mem::take(&mut stored.0));
                    values.record(&mut c);
                    stored.0 = c.0;
                }
            }
        }

        fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
            let mut c = Collect::default();
            event.record(&mut c);
            let mut spans = Vec::new();
            if let Some(scope) = ctx.event_scope(event) {
                for span in scope {
                    let fields = span.extensions().get::<Stored>().map(|s| s.0.clone()).unwrap_or_default();
                    spans.push(SpanRec { name: span.name().to_string(), fields });
                }
            }
            let meta = event.metadata();
            self.events.lock().unwrap().push(Rec { level: *meta.level(), target: meta.target().to_string(), fields: c.0, spans });
        }

        fn on_close(&self, id: Id, ctx: Context<'_, S>) {
            if let Some(span) = ctx.span(&id) {
                let fields = span.extensions().get::<Stored>().map(|s| s.0.clone()).unwrap_or_default();
                self.closed.lock().unwrap().push(SpanRec { name: span.name().to_string(), fields });
            }
        }
    }

    pub fn install() -> (Capture, tracing::subscriber::DefaultGuard) {
        let cap = Capture::default();
        let guard = tracing::subscriber::set_default(tracing_subscriber::registry().with(cap.clone()));
        (cap, guard)
    }

    pub fn capture<R>(run: impl FnOnce() -> R) -> (R, Capture) {
        let cap = Capture::default();
        let result = tracing::subscriber::with_default(tracing_subscriber::registry().with(cap.clone()), run);
        (result, cap)
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::*;

    fn req(id: u64, path: &str, user: u64) -> Request {
        Request { id, method: "GET", path: path.to_string(), user }
    }

    #[tokio::test]
    async fn every_event_carries_the_context_of_its_own_request() {
        let (cap, _guard) = install();
        let (a, b, c) = tokio::join!(handle(req(101, "/feed", 1)), handle(req(102, "/feed/new", 2)), handle(req(103, "/nope", 3)));
        assert_eq!((a, b, c), (200, 200, 404));
        let owner = |user: i128| 100 + user;
        let events = cap.events();
        assert_eq!(events.len(), 3 + 3 + 2, "помощники и serve пишут по событию: восемь штук на три запроса");
        for e in events {
            let user = e.int("user").expect("у события есть поле user");
            let span = e.span("request").unwrap_or_else(|| panic!("событие «{}» лежит вне span `request`", e.message()));
            assert_eq!(span.int("request_id"), Some(owner(user)), "событие «{}» пользователя {user} получило контекст чужого запроса: {:?}", e.message(), span.fields);
        }
    }

    #[tokio::test]
    async fn the_span_has_method_and_path() {
        let (cap, _guard) = install();
        handle(req(7, "/feed?page=2", 5)).await;
        let spans = cap.closed();
        let span = spans.iter().find(|s| s.name == "request").expect("span `request` создан и закрыт");
        assert_eq!(span.text("method").as_deref(), Some("GET"));
        assert_eq!(span.text("path").as_deref(), Some("/feed?page=2"), "path должен быть строкой: {:?}", span.fields);
        assert_eq!(span.int("request_id"), Some(7));
    }

    #[tokio::test]
    async fn the_status_is_filled_in_when_the_request_is_done() {
        let (cap, _guard) = install();
        let (ok, missing) = tokio::join!(handle(req(1, "/feed", 1)), handle(req(2, "/zzz", 2)));
        assert_eq!((ok, missing), (200, 404));
        let spans = cap.closed();
        let status_of = |id: i128| spans.iter().find(|s| s.name == "request" && s.int("request_id") == Some(id)).and_then(|s| s.int("status"));
        assert_eq!(status_of(1), Some(200), "в закрытом span запроса 1 должен быть status = 200: {spans:?}");
        assert_eq!(status_of(2), Some(404), "в закрытом span запроса 2 должен быть status = 404");
    }

    #[tokio::test]
    async fn one_span_per_request_and_none_left_open() {
        let (cap, _guard) = install();
        for i in 0..5 {
            handle(req(i, "/feed", i)).await;
        }
        let closed = cap.closed().into_iter().filter(|s| s.name == "request").count();
        assert_eq!(closed, 5, "по одному span на запрос, и все они закрыты после ответа");
    }
}
