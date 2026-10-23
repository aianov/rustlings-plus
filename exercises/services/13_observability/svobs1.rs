// Лента постов пишет в лог одну строку на событие: «user 7 published post 42 from 10.0.0.1». Человеку читать удобно, а
// поиску по логам нет: чтобы найти все публикации автора 7, нужна регулярка, и она сломается, когда кто-то поменяет слова.
// Цель: данные события лежат в полях, а текст сообщения одинаков при любой публикации и любом отказе. Публикация: INFO с
// полями `author_id`, `post_id`, `ip`. Отказ (пустой или слишком длинный пост): WARN с `author_id`, `reason` («empty» или
// «too_long») и `len`. Сам текст поста в логи не попадает ни в каком виде.


use tracing::{info, warn};

fn main() {}

pub const MAX_LEN: usize = 280;

#[derive(Debug, PartialEq)]
pub enum PublishError {
    Empty,
    TooLong,
}

#[derive(Default)]
pub struct Feed {
    posts: Vec<(u64, u64, String)>,
}

impl Feed {
    pub fn len(&self) -> usize {
        self.posts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.posts.is_empty()
    }

    pub fn publish(&mut self, author_id: u64, text: &str, ip: &str) -> Result<u64, PublishError> {
        let len = text.chars().count();
        if len == 0 {
            warn!("user {author_id} tried to publish an empty post");
            return Err(PublishError::Empty);
        }
        if len > MAX_LEN {
            warn!("user {author_id} tried to publish a post of {len} chars: {text}");
            return Err(PublishError::TooLong);
        }
        let post_id = self.posts.len() as u64 + 1;
        self.posts.push((post_id, author_id, text.to_string()));
        info!("user {author_id} published post {post_id} from {ip}");
        Ok(post_id)
    }
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
    use tracing::Level;

    #[test]
    fn a_published_post_is_an_info_event_with_the_data_in_fields() {
        let (id, cap) = capture(|| Feed::default().publish(7, "привет, мир", "10.0.0.1"));
        assert_eq!(id, Ok(1));
        let events = cap.events();
        assert_eq!(events.len(), 1, "публикация пишет ровно одно событие, а не по строке на каждый шаг");
        let e = &events[0];
        assert_eq!(e.level, Level::INFO, "успешная публикация это INFO");
        assert_eq!(e.int("author_id"), Some(7), "поле author_id должно быть числом, а не частью текста: {:?}", e.fields);
        assert_eq!(e.int("post_id"), Some(1), "поле post_id должно быть числом: {:?}", e.fields);
        assert_eq!(e.text("ip").as_deref(), Some("10.0.0.1"), "поле ip должно быть строкой: {:?}", e.fields);
    }

    #[test]
    fn the_message_text_is_the_same_for_every_publication() {
        let (_, cap) = capture(|| {
            let mut feed = Feed::default();
            feed.publish(7, "первый", "10.0.0.1").unwrap();
            feed.publish(8, "второй", "10.0.0.2").unwrap();
        });
        let events = cap.events();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].message(), events[1].message(), "в тексте сообщения остались данные ({:?} и {:?}): по такому тексту не сгруппировать события", events[0].message(), events[1].message());
        assert!(!events[0].message().is_empty(), "у события должен быть читаемый текст");
    }

    #[test]
    fn a_rejected_post_is_a_warning_with_the_reason_and_length() {
        let (results, cap) = capture(|| {
            let mut feed = Feed::default();
            (feed.publish(7, "", "10.0.0.1"), feed.publish(9, &"я".repeat(MAX_LEN + 20), "10.0.0.1"))
        });
        assert_eq!(results, (Err(PublishError::Empty), Err(PublishError::TooLong)));
        let events = cap.events();
        assert_eq!(events.len(), 2, "каждый отказ это одно событие");
        for e in &events {
            assert_eq!(e.level, Level::WARN, "отказ клиенту это WARN: сервис сработал верно, а запрос был плохим");
        }
        assert_eq!(events[0].text("reason").as_deref(), Some("empty"));
        assert_eq!(events[0].int("author_id"), Some(7));
        assert_eq!(events[0].int("len"), Some(0));
        assert_eq!(events[1].text("reason").as_deref(), Some("too_long"));
        assert_eq!(events[1].int("author_id"), Some(9));
        assert_eq!(events[1].int("len"), Some((MAX_LEN + 20) as i128), "len считается в символах, а не в байтах: {:?}", events[1].fields);
        assert_eq!(events[0].message(), events[1].message(), "причина в поле reason, а текст сообщения общий");
    }

    #[test]
    fn the_text_of_the_post_never_reaches_the_log() {
        let secret = "мой секретный пароль hunter2";
        let long = format!("{secret}{}", "!".repeat(MAX_LEN));
        let (_, cap) = capture(|| {
            let mut feed = Feed::default();
            feed.publish(1, secret, "10.0.0.1").unwrap();
            let _ = feed.publish(1, &long, "10.0.0.1");
        });
        for e in cap.events() {
            let dump = format!("{:?}", e.fields);
            assert!(!dump.contains("hunter2"), "текст поста попал в лог: {dump}");
        }
    }

    #[test]
    fn nothing_is_logged_by_reading() {
        let (_, cap) = capture(|| {
            let feed = Feed::default();
            (feed.len(), feed.is_empty())
        });
        assert!(cap.events().is_empty(), "чтение ленты ничего не пишет в лог");
    }
}
