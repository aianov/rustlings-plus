// Сервис стартует, читает переменную окружения `RUST_LOG` и настраивает, что попадёт в лог. Без неё должен работать разумный
// режим: наши сообщения от INFO и выше, а болтливые `hyper`, `h2` и `tower` только от WARN. С ней работает то, что написал
// оператор («debug» или «warn,social::posts=debug»). Опечатка в `RUST_LOG` не должна ронять сервис в три часа ночи или молча
// делать его немым: тогда включается режим по умолчанию, а текст проблемы возвращается, чтобы записать его в лог после запуска.
// Цель: `make_filter` возвращает фильтр и, если значение было, но не разобралось, описание проблемы со словом из значения.


use tracing_subscriber::EnvFilter;

fn main() {}

pub fn make_filter(rust_log: Option<&str>) -> (EnvFilter, Option<String>) {
    let _ = rust_log;
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
    use tracing::{debug, info, warn};
    use tracing_subscriber::Layer;
    use tracing_subscriber::layer::SubscriberExt;

    fn emit_all() {
        debug!(target: "social::posts", "d-posts");
        info!(target: "social::posts", "i-posts");
        info!(target: "social::feed", "i-feed");
        debug!(target: "hyper::proto", "d-hyper");
        info!(target: "hyper::proto", "i-hyper");
        warn!(target: "hyper::proto", "w-hyper");
    }

    fn seen(rust_log: Option<&str>) -> (Vec<String>, Option<String>) {
        let (filter, problem) = make_filter(rust_log);
        let cap = Capture::default();
        let subscriber = tracing_subscriber::registry().with(cap.clone().with_filter(filter));
        tracing::subscriber::with_default(subscriber, emit_all);
        (cap.events().iter().map(|e| e.message()).collect(), problem)
    }

    #[test]
    fn without_the_variable_our_info_and_noisy_libraries_only_warn() {
        let (messages, problem) = seen(None);
        assert_eq!(messages, ["i-posts", "i-feed", "w-hyper"], "по умолчанию: свои сообщения от INFO, hyper только от WARN");
        assert_eq!(problem, None, "отсутствие переменной не проблема");
    }

    #[test]
    fn an_empty_value_is_the_same_as_no_value() {
        for value in ["", "   "] {
            let (messages, problem) = seen(Some(value));
            assert_eq!(messages, ["i-posts", "i-feed", "w-hyper"], "значение {value:?} это то же, что нет значения");
            assert_eq!(problem, None, "пустое значение не проблема, о нём не надо ничего сообщать");
        }
    }

    #[test]
    fn a_plain_level_applies_to_everything() {
        let (messages, problem) = seen(Some("debug"));
        assert_eq!(messages, ["d-posts", "i-posts", "i-feed", "d-hyper", "i-hyper", "w-hyper"], "оператор попросил debug для всего, включая hyper");
        assert_eq!(problem, None);
    }

    #[test]
    fn a_directive_can_raise_one_module_above_the_rest() {
        let (messages, _) = seen(Some("warn,social::posts=debug"));
        assert_eq!(messages, ["d-posts", "i-posts", "w-hyper"], "social::posts на debug, остальное на warn");
    }

    #[test]
    fn off_silences_everything() {
        let (messages, problem) = seen(Some("off"));
        assert!(messages.is_empty(), "off значит ни одного события: {messages:?}");
        assert_eq!(problem, None);
    }

    #[test]
    fn a_typo_falls_back_to_the_default_and_reports_what_was_wrong() {
        let (messages, problem) = seen(Some("social::posts=loud"));
        assert_eq!(messages, ["i-posts", "i-feed", "w-hyper"], "из-за опечатки сервис не должен стать немым: нужен режим по умолчанию");
        let text = problem.expect("опечатку надо вернуть как описание проблемы");
        assert!(text.contains("loud"), "в описании проблемы должно быть само значение или слово из него: {text}");
    }
}
