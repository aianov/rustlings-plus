// Дежурному нужна страница «последние сбои»: все события уровня WARN и ERROR с их полями, чтобы не искать их в потоке логов.
// Для этого пишется свой `Layer`: он подключается к подписчику рядом с обычным выводом и складывает записи в общий список.
// Цель: `ErrorLog` запоминает каждое событие уровня WARN и ERROR (INFO, DEBUG и TRACE пропускает) как `Entry` с уровнем,
// целью, текстом сообщения и остальными полями в виде строк. Строки должны быть такими, как их увидит человек: без
// кавычек вокруг текста и с текстом ошибки, а не с отладочным дампом.


use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;

fn main() {}

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub level: Level,
    pub target: String,
    pub message: String,
    pub fields: BTreeMap<String, String>,
}

#[derive(Clone, Default)]
pub struct ErrorLog {
    entries: Arc<Mutex<Vec<Entry>>>,
}

impl ErrorLog {
    pub fn entries(&self) -> Vec<Entry> {
        self.entries.lock().expect("список записей не отравлен").clone()
    }
}

impl<S: Subscriber> Layer<S> for ErrorLog {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let _ = event;
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing::{debug, error, info, warn};
    use tracing_subscriber::layer::SubscriberExt;

    fn run(f: impl FnOnce()) -> Vec<Entry> {
        let log = ErrorLog::default();
        tracing::subscriber::with_default(tracing_subscriber::registry().with(log.clone()), f);
        log.entries()
    }

    #[test]
    fn only_warnings_and_errors_are_kept() {
        let entries = run(|| {
            debug!("отладка");
            info!("обычное");
            warn!("странное");
            error!("плохое");
        });
        let levels: Vec<Level> = entries.iter().map(|e| e.level).collect();
        assert_eq!(levels, [Level::WARN, Level::ERROR], "INFO и DEBUG в список сбоев не попадают, WARN и ERROR попадают");
    }

    #[test]
    fn the_message_is_plain_text_and_fields_are_strings_as_a_human_sees_them() {
        let entries = run(|| {
            error!(target: "pay", order = 77, attempts = 3u32, ok = false, card = "visa", "оплата не прошла");
        });
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert_eq!(e.message, "оплата не прошла", "сообщение без кавычек и без служебных символов");
        assert_eq!(e.target, "pay");
        assert_eq!(e.fields.get("order").map(String::as_str), Some("77"), "число записывается цифрами: {:?}", e.fields);
        assert_eq!(e.fields.get("attempts").map(String::as_str), Some("3"));
        assert_eq!(e.fields.get("ok").map(String::as_str), Some("false"));
        assert_eq!(e.fields.get("card").map(String::as_str), Some("visa"), "строковое поле без кавычек: {:?}", e.fields);
        assert!(!e.fields.contains_key("message"), "message не поле, а текст события");
    }

    #[test]
    fn an_error_value_is_written_as_its_text() {
        let entries = run(|| {
            let failure = "12x".parse::<u32>().unwrap_err();
            warn!(error = &failure as &(dyn std::error::Error + 'static), "не разобрали число");
        });
        let shown = entries[0].fields.get("error").cloned().unwrap_or_default();
        assert_eq!(shown, "invalid digit found in string", "ошибка должна показываться текстом (Display), а не структурой Debug");
    }

    #[test]
    fn display_and_debug_wrappers_keep_their_own_format() {
        let entries = run(|| {
            warn!(who = %"Аня", shape = ?(1, 2), "поля с оболочками");
        });
        assert_eq!(entries[0].fields.get("who").map(String::as_str), Some("Аня"));
        assert_eq!(entries[0].fields.get("shape").map(String::as_str), Some("(1, 2)"));
    }

    #[test]
    fn entries_arrive_in_order_and_survive_many_events() {
        let entries = run(|| {
            for i in 0..50 {
                warn!(i, "сбой");
            }
        });
        let numbers: Vec<String> = entries.iter().map(|e| e.fields["i"].clone()).collect();
        let expected: Vec<String> = (0..50).map(|i| i.to_string()).collect();
        assert_eq!(numbers, expected, "записи идут в порядке событий, ничего не теряется");
    }
}
