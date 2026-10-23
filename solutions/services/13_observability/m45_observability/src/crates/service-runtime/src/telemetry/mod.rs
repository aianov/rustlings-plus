use std::fmt::{self, Write as _};
use std::io::Write as _;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::{EnvFilter, Registry};

pub const LOG_ENV: &str = "RUST_LOG";

pub const HIDDEN: &str = "[скрыто]";

pub const SECRET_FIELDS: [&str; 7] = [
    "authorization",
    "token",
    "password",
    "cookie",
    "secret",
    "access_token",
    "refresh_token",
];

type Write = Arc<dyn Fn(Level, &str) + Send + Sync>;

#[derive(Debug, Clone, PartialEq)]
enum Value {
    Text(String),
    Raw(String),
}

#[derive(Debug, Default, Clone)]
struct Fields(Vec<(String, Value)>);

fn secret(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    SECRET_FIELDS
        .iter()
        .any(|known| name == *known || name.ends_with(&format!("_{known}")))
}

fn scrub(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    match lower.find("bearer ") {
        Some(at) => format!("{}Bearer {HIDDEN}", &text[..at]),
        None => text.to_string(),
    }
}

impl Fields {
    fn put(&mut self, field: &Field, value: Value) {
        let name = field.name().to_string();
        let value = if secret(&name) {
            Value::Text(HIDDEN.to_string())
        } else {
            match value {
                Value::Text(text) => Value::Text(scrub(&text)),
                raw => raw,
            }
        };
        self.0.retain(|(own, _)| *own != name);
        self.0.push((name, value));
    }
}

impl Visit for Fields {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.put(field, Value::Text(value.to_string()));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.put(field, Value::Raw(value.to_string()));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.put(field, Value::Raw(value.to_string()));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.put(field, Value::Raw(value.to_string()));
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        let raw = if value.is_finite() {
            value.to_string()
        } else {
            format!("\"{value}\"")
        };
        self.put(field, Value::Raw(raw));
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.put(field, Value::Text(format!("{value:?}")));
    }
}

pub fn escape_json(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 || c == '\u{2028}' || c == '\u{2029}' => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out
}

const RESERVED: [&str; 4] = ["ts", "level", "service", "target"];

fn push(line: &mut String, name: &str, value: &Value) {
    let name = if RESERVED.contains(&name) {
        format!("field_{name}")
    } else {
        name.to_string()
    };
    let _ = write!(line, ",\"{}\":", escape_json(&name));
    match value {
        Value::Text(text) => {
            let _ = write!(line, "\"{}\"", escape_json(text));
        }
        Value::Raw(raw) => line.push_str(raw),
    }
}

#[derive(Clone)]
pub struct JsonLayer {
    service: &'static str,
    write: Write,
}

impl JsonLayer {
    pub fn new(
        service: &'static str,
        write: impl Fn(Level, &str) + Send + Sync + 'static,
    ) -> JsonLayer {
        JsonLayer {
            service,
            write: Arc::new(write),
        }
    }

    pub fn standard(service: &'static str) -> JsonLayer {
        JsonLayer::new(service, |level, line| {
            if level <= Level::WARN {
                let _ = writeln!(std::io::stderr().lock(), "{line}");
            } else {
                let _ = writeln!(std::io::stdout().lock(), "{line}");
            }
        })
    }
}

impl fmt::Debug for JsonLayer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "JsonLayer({})", self.service)
    }
}

impl<S> Layer<S> for JsonLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let mut fields = Fields::default();
        attrs.record(&mut fields);
        if let Some(span) = ctx.span(id) {
            span.extensions_mut().insert(fields);
        }
    }

    fn on_record(&self, id: &Id, values: &Record<'_>, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(id) {
            let mut extensions = span.extensions_mut();
            if let Some(fields) = extensions.get_mut::<Fields>() {
                values.record(fields);
            }
        }
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let mut fields = Fields::default();
        event.record(&mut fields);
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_millis());
        let meta = event.metadata();
        let mut line = format!(
            "{{\"ts\":{millis},\"level\":\"{}\",\"service\":\"{}\",\"target\":\"{}\"",
            meta.level(),
            escape_json(self.service),
            escape_json(meta.target())
        );
        let mut spans = Vec::new();
        let mut context = Fields::default();
        if let Some(scope) = ctx.event_scope(event) {
            for span in scope.from_root() {
                spans.push(span.name().to_string());
                if let Some(own) = span.extensions().get::<Fields>() {
                    for (name, value) in &own.0 {
                        context.0.retain(|(seen, _)| seen != name);
                        context.0.push((name.clone(), value.clone()));
                    }
                }
            }
        }
        if let Some((_, message)) = fields.0.iter().find(|(name, _)| name == "message") {
            push(&mut line, "message", message);
        }
        for (name, value) in fields.0.iter().filter(|(name, _)| name != "message") {
            push(&mut line, name, value);
        }
        for (name, value) in &context.0 {
            if !fields.0.iter().any(|(own, _)| own == name) {
                push(&mut line, name, value);
            }
        }
        if !spans.is_empty() {
            push(&mut line, "spans", &Value::Text(spans.join(">")));
        }
        line.push('}');
        (self.write)(*meta.level(), &line);
    }
}

pub fn filter() -> EnvFilter {
    EnvFilter::try_from_env(LOG_ENV).unwrap_or_else(|_| EnvFilter::new("info"))
}

pub fn init_logging(service: &'static str) -> bool {
    let subscriber = Registry::default()
        .with(filter())
        .with(JsonLayer::standard(service));
    tracing::subscriber::set_global_default(subscriber).is_ok()
}
