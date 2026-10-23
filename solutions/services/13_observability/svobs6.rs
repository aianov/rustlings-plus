
use std::sync::{Arc, Mutex};
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;

fn main() {}

#[derive(Clone, Default)]
pub struct JsonLines {
    lines: Arc<Mutex<Vec<String>>>,
}

impl JsonLines {
    pub fn lines(&self) -> Vec<String> {
        self.lines.lock().expect("список строк не отравлен").clone()
    }
}

use serde_json::{Map, Number, Value};

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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use tracing::{info, info_span, warn};
    use tracing_subscriber::layer::SubscriberExt;

    fn run(f: impl FnOnce()) -> Vec<String> {
        let sink = JsonLines::default();
        tracing::subscriber::with_default(tracing_subscriber::registry().with(sink.clone()), f);
        sink.lines()
    }

    fn parse(line: &str) -> Value {
        serde_json::from_str(line).unwrap_or_else(|e| panic!("строка не разбирается как JSON ({e}): {line}"))
    }

    #[test]
    fn a_plain_event_has_level_target_message_and_typed_fields() {
        let lines = run(|| warn!(target: "billing", user = 7, ratio = 0.5, paid = true, plan = "pro", "счёт выставлен"));
        assert_eq!(lines.len(), 1);
        let v = parse(&lines[0]);
        assert_eq!(v["level"], "WARN");
        assert_eq!(v["target"], "billing");
        assert_eq!(v["message"], "счёт выставлен");
        assert_eq!(v["fields"]["user"], 7, "число должно остаться числом: {}", lines[0]);
        assert_eq!(v["fields"]["ratio"], 0.5);
        assert_eq!(v["fields"]["paid"], true, "булево должно остаться булевым: {}", lines[0]);
        assert_eq!(v["fields"]["plan"], "pro");
    }

    #[test]
    fn quotes_backslashes_and_newlines_in_the_message_survive() {
        let nasty = "он сказал \"привет\"\nи ушёл\tпо пути C:\\temp\r\n";
        let lines = run(|| info!("{nasty}"));
        assert!(!lines[0].contains('\n') && !lines[0].contains('\r'), "перевод строки внутри данных разорвал бы строку лога надвое: {:?}", lines[0]);
        assert_eq!(parse(&lines[0])["message"], nasty, "после разбора сообщение должно совпасть с исходным");
    }

    #[test]
    fn control_characters_and_unicode_in_field_values() {
        let value = "звонок\u{7} и NUL\u{0} и эмодзи 🙂 и \u{1f}";
        let lines = run(|| info!(note = value, "поле с управляющими символами"));
        assert!(lines[0].chars().all(|c| c >= ' '), "в строке остались необработанные управляющие символы: {:?}", lines[0]);
        assert_eq!(parse(&lines[0])["fields"]["note"], value);
    }

    #[test]
    fn a_nan_does_not_break_the_line() {
        let lines = run(|| info!(score = f64::NAN, big = u64::MAX, "числа на краю"));
        let v = parse(&lines[0]);
        assert_eq!(v["fields"]["score"], Value::Null, "NaN в JSON недопустим, вместо него null");
        assert_eq!(v["fields"]["big"], u64::MAX, "большое число не теряет точность");
    }

    #[test]
    fn spans_are_listed_from_the_outermost_to_the_innermost_with_their_fields() {
        let lines = run(|| {
            let request = info_span!("request", request_id = 5, path = "/feed");
            let _outer = request.enter();
            let step = info_span!("db", table = "posts");
            let _inner = step.enter();
            info!("запрос выполнен");
        });
        let v = parse(&lines[0]);
        let spans = v["spans"].as_array().expect("spans это массив");
        assert_eq!(spans.len(), 2, "оба span вокруг события должны быть в строке: {}", lines[0]);
        assert_eq!(spans[0]["name"], "request", "сначала внешний");
        assert_eq!(spans[0]["fields"]["request_id"], 5);
        assert_eq!(spans[0]["fields"]["path"], "/feed");
        assert_eq!(spans[1]["name"], "db");
        assert_eq!(spans[1]["fields"]["table"], "posts");
    }

    #[test]
    fn fields_recorded_into_a_span_later_are_visible() {
        let lines = run(|| {
            let span = info_span!("request", status = tracing::field::Empty);
            let _g = span.enter();
            span.record("status", 503);
            info!("ответили");
        });
        assert_eq!(parse(&lines[0])["spans"][0]["fields"]["status"], 503, "поле, записанное в span позже, должно попасть в строку: {}", lines[0]);
    }

    #[test]
    fn an_event_outside_any_span_has_an_empty_spans_array() {
        let lines = run(|| info!("без контекста"));
        assert_eq!(parse(&lines[0])["spans"], serde_json::json!([]));
    }
}
