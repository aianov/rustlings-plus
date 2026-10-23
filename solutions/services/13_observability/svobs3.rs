
use tokio::task::yield_now;
use tracing::{info, info_span};
use tracing::Instrument;

fn main() {}

async fn fetch(job: u64) {
    yield_now().await;
    info!(job, "данные получены");
}

async fn notify_followers(job: u64) {
    yield_now().await;
    info!(job, "рассылка выполнена");
}

pub async fn process(job: u64) {
    async move {
        fetch(job).await;
        let sender = tokio::spawn(notify_followers(job).in_current_span());
        sender.await.expect("рассылка не должна падать");
        info!(job, "готово");
    }
    .instrument(info_span!("job", job_id = job))
    .await
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
    use tracing_subscriber::layer::SubscriberExt;

    #[tokio::test]
    async fn events_of_concurrent_jobs_keep_their_own_job_id() {
        let (cap, _guard) = install();
        tokio::join!(process(1), process(2), process(3));
        let events = cap.events();
        assert_eq!(events.len(), 9, "три шага на каждую из трёх задач");
        for e in events {
            let job = e.int("job").expect("у события есть поле job");
            let span = e.span("job").unwrap_or_else(|| panic!("событие «{}» задачи {job} лежит вне span `job`", e.message()));
            assert_eq!(span.int("job_id"), Some(job), "событие «{}» задачи {job} записано в span чужой задачи: {:?}", e.message(), span.fields);
        }
    }

    fn global() -> &'static Capture {
        static CAP: std::sync::OnceLock<Capture> = std::sync::OnceLock::new();
        CAP.get_or_init(|| {
            let cap = Capture::default();
            tracing::subscriber::set_global_default(tracing_subscriber::registry().with(cap.clone())).expect("глобальный подписчик ставится один раз");
            cap
        })
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_spawned_notification_still_knows_its_job() {
        let cap = global();
        process(4242).await;
        let events = cap.events();
        let sent = events.iter().find(|e| e.message() == "рассылка выполнена" && e.int("job") == Some(4242)).expect("рассылка отработала");
        let span = sent.span("job").expect("событие из запущенной задачи потеряло контекст: tokio::spawn не переносит span сам");
        assert_eq!(span.int("job_id"), Some(4242));
    }

    #[tokio::test]
    async fn a_job_leaves_no_span_open_and_the_next_event_is_clean() {
        let (cap, _guard) = install();
        process(5).await;
        info!("после задачи");
        let events = cap.events();
        let last = events.last().expect("событие после задачи записано");
        assert!(last.spans.is_empty(), "после завершения задачи span остался текущим: {:?}", last.spans);
        assert_eq!(cap.closed().iter().filter(|s| s.name == "job").count(), 1, "span задачи закрыт ровно один раз");
    }

    #[tokio::test]
    async fn the_work_itself_is_unchanged() {
        let (cap, _guard) = install();
        let both = tokio::join!(process(8), process(9));
        assert_eq!(both, ((), ()));
        let messages: Vec<String> = cap.events().iter().filter(|e| e.int("job") == Some(8)).map(|e| e.message()).collect();
        assert_eq!(messages, ["данные получены", "рассылка выполнена", "готово"], "порядок шагов задачи 8 не должен измениться");
    }
}
