
use serde_json::{Map, Number, Value};
use std::sync::{Arc, Mutex};
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;

#[derive(Clone, Default)]
pub struct JsonLines {
    lines: Arc<Mutex<Vec<String>>>,
}

impl JsonLines {
    pub fn lines(&self) -> Vec<String> {
        self.lines.lock().expect("список строк не отравлен").clone()
    }
}

struct Stored(Map<String, Value>);

struct Parts {
    message: String,
    fields: Map<String, Value>,
}

impl Parts {
    fn new() -> Parts {
        Parts { message: String::new(), fields: Map::new() }
    }
}

impl Visit for Parts {
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.fields.insert(field.name().to_string(), Value::from(value));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.fields.insert(field.name().to_string(), Value::from(value));
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        let json = Number::from_f64(value).map_or(Value::Null, Value::Number);
        self.fields.insert(field.name().to_string(), json);
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.fields.insert(field.name().to_string(), Value::Bool(value));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        } else {
            self.fields.insert(field.name().to_string(), Value::String(value.to_string()));
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let text = format!("{value:?}");
        if field.name() == "message" {
            self.message = text;
        } else {
            self.fields.insert(field.name().to_string(), Value::String(text));
        }
    }
}

impl<S> Layer<S> for JsonLines
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let mut parts = Parts::new();
        attrs.record(&mut parts);
        if let Some(span) = ctx.span(id) {
            span.extensions_mut().insert(Stored(parts.fields));
        }
    }

    fn on_record(&self, id: &Id, values: &Record<'_>, ctx: Context<'_, S>) {
        let mut parts = Parts::new();
        values.record(&mut parts);
        if let Some(span) = ctx.span(id) {
            if let Some(Stored(map)) = span.extensions_mut().get_mut::<Stored>() {
                map.extend(parts.fields);
            }
        }
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let mut parts = Parts::new();
        event.record(&mut parts);
        let mut spans = Vec::new();
        if let Some(scope) = ctx.event_scope(event) {
            for span in scope.from_root() {
                let fields = span.extensions().get::<Stored>().map(|s| s.0.clone()).unwrap_or_default();
                spans.push(serde_json::json!({ "name": span.name(), "fields": fields }));
            }
        }
        let meta = event.metadata();
        let line = serde_json::json!({
            "level": meta.level().as_str(),
            "target": meta.target(),
            "message": parts.message,
            "fields": parts.fields,
            "spans": spans,
        });
        self.lines.lock().expect("список строк не отравлен").push(line.to_string());
    }
}
