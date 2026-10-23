use access_log::*;
use std::alloc::{GlobalAlloc, Layout, System};
use std::borrow::Cow;
use std::cell::Cell;

struct Counting;

thread_local! {
    static ALLOCS: Cell<usize> = const { Cell::new(0) };
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCS.with(|c| c.set(c.get() + 1));
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCS.with(|c| c.set(c.get() + 1));
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static COUNTING: Counting = Counting;

fn allocations<R>(f: impl FnOnce() -> R) -> (R, usize) {
    let before = ALLOCS.with(Cell::get);
    let result = f();
    (result, ALLOCS.with(Cell::get) - before)
}

const SAMPLE: &str = "203.0.113.7 GET /feed?page=2 200 \"Mozilla/5.0 (X11)\"
198.51.100.4 POST /login 401 \"curl/8.0\"

203.0.113.7 GET /post/17 404 \"Say \\\"hi\\\" bot\"
192.0.2.1 GET /feed?page=3&author=ann 500 \"back\\\\slash\"
";

fn sample() -> AccessLog {
    AccessLog::new(SAMPLE.to_string()).expect("пример разбирается без ошибок")
}

fn inside(log: &AccessLog, piece: &str) -> bool {
    let start = log.raw().as_ptr() as usize;
    let at = piece.as_ptr() as usize;
    at >= start && at + piece.len() <= start + log.raw().len()
}

#[test]
fn parses_every_field_and_skips_blank_lines() {
    let log = sample();
    assert_eq!(log.len(), 4, "пустая строка записью не считается");
    assert!(!log.is_empty());
    let first = log.get(0).expect("первая запись");
    assert_eq!((first.ip, first.method, first.target, first.status), ("203.0.113.7", "GET", "/feed?page=2", 200));
    assert_eq!(first.agent, "Mozilla/5.0 (X11)");
    let third = log.get(2).expect("третья запись");
    assert_eq!((third.status, third.target), (404, "/post/17"));
    assert!(log.get(4).is_none());
}

#[test]
fn escapes_in_the_agent_are_decoded() {
    let log = sample();
    assert_eq!(log.get(2).map(|e| e.agent.into_owned()), Some("Say \"hi\" bot".to_string()), "\\\" внутри кавычек превращается в \"");
    assert_eq!(log.get(3).map(|e| e.agent.into_owned()), Some("back\\slash".to_string()), "\\\\ превращается в одну \\");
}

#[test]
fn agents_without_escapes_are_borrowed_from_the_buffer() {
    let log = sample();
    let plain = log.get(0).expect("запись");
    assert!(matches!(plain.agent, Cow::Borrowed(_)), "без экранирования копировать не нужно: Cow::Borrowed");
    assert!(inside(&log, &plain.agent), "агент должен быть куском буфера");
    assert!(matches!(log.get(2).expect("запись").agent, Cow::Owned(_)), "с экранированием приходится собирать новую строку");
}

#[test]
fn text_fields_are_slices_of_the_buffer() {
    let log = sample();
    for entry in log.entries() {
        assert!(inside(&log, entry.ip) && inside(&log, entry.method) && inside(&log, entry.target), "ip, метод и цель режутся из буфера без копий");
    }
}

#[test]
fn path_and_query_split_the_target() {
    let log = sample();
    let feed = log.get(0).expect("запись");
    assert_eq!((feed.path(), feed.query()), ("/feed", Some("page=2")));
    let login = log.get(1).expect("запись");
    assert_eq!((login.path(), login.query()), ("/login", None));
}

#[test]
fn path_and_query_outlive_the_entry() {
    let log = sample();
    let (path, query) = {
        let entry = log.get(3).expect("запись");
        (entry.path(), entry.query())
    };
    assert_eq!((path, query), ("/feed", Some("page=3&author=ann")), "куски живут столько же, сколько журнал, а не запись");
}

#[test]
fn entries_come_in_file_order_and_status_ranges_filter() {
    let log = sample();
    let all: Vec<u16> = log.entries().map(|e| e.status).collect();
    assert_eq!(all, vec![200, 401, 404, 500]);
    let client_errors: Vec<u16> = log.with_status(400, 499).map(|e| e.status).collect();
    assert_eq!(client_errors, vec![401, 404]);
    assert_eq!(log.with_status(300, 399).count(), 0);
}

#[test]
fn bad_lines_report_their_line_number() {
    let missing = AccessLog::new("1.1.1.1 GET /\n".to_string());
    assert_eq!(missing.err(), Some(LogError { line: 1, reason: "missing field" }));
    let status = AccessLog::new("1.1.1.1 GET / 200 \"ok\"\n\n1.1.1.1 GET / abc \"ok\"\n".to_string());
    assert_eq!(status.err(), Some(LogError { line: 3, reason: "bad status" }), "номер строки считается вместе с пустыми");
    let open = AccessLog::new("1.1.1.1 GET / 200 \"never closed\n".to_string());
    assert_eq!(open.err(), Some(LogError { line: 1, reason: "bad agent" }));
    let tail = AccessLog::new("1.1.1.1 GET / 200 \"ok\" junk\n".to_string());
    assert_eq!(tail.err(), Some(LogError { line: 1, reason: "bad agent" }), "после закрывающей кавычки ничего быть не должно");
    let no_agent = AccessLog::new("1.1.1.1 GET / 200\n".to_string());
    assert_eq!(no_agent.err(), Some(LogError { line: 1, reason: "bad agent" }), "четыре поля есть, агента нет: это bad agent, а не missing field");
    let blanks = AccessLog::new("   \n1.1.1.1 GET / 200 \"ok\"\n".to_string());
    assert_eq!(blanks.map(|l| l.len()).ok(), Some(1), "строка из одних пробелов пропускается как пустая");
    let unquoted = AccessLog::new("1.1.1.1 GET / 200 ok\n".to_string());
    assert_eq!(unquoted.err(), Some(LogError { line: 1, reason: "bad agent" }));
}

#[test]
fn last_line_may_lack_a_newline_and_windows_endings_work() {
    let log = AccessLog::new("1.1.1.1 GET / 200 \"a\"\r\n2.2.2.2 GET /x 201 \"b\"".to_string()).expect("разбор");
    assert_eq!(log.len(), 2);
    assert_eq!(log.get(1).map(|e| e.ip), Some("2.2.2.2"));
    assert_eq!(log.get(0).map(|e| e.agent.into_owned()), Some("a".to_string()), "\\r не попадает в запись");
}

#[test]
fn the_log_can_be_moved_without_breaking_entries() {
    let logs = vec![sample(), sample()];
    let moved = logs;
    let entry = moved[1].get(0).expect("запись");
    assert_eq!(entry.ip, "203.0.113.7");
    assert!(inside(&moved[1], entry.ip), "после перемещения журнала записи по-прежнему указывают в его буфер");
}

fn big() -> String {
    let mut text = String::new();
    for i in 0..20_000 {
        text.push_str(&format!("10.0.{}.{} GET /page/{i} {} \"agent {i}\"\n", i % 250, i % 200, 200 + (i % 4) * 100));
    }
    text
}

#[test]
fn building_the_log_allocates_a_constant_amount() {
    let text = big();
    let (log, allocs) = allocations(|| AccessLog::new(text));
    let log = log.expect("разбор");
    assert_eq!(log.len(), 20_000);
    assert!(allocs <= 6, "аллокаций {allocs}: по записи на строку не нужно, хватает вектора диапазонов (заведи его сразу нужной ёмкости)");
}

#[test]
fn walking_the_log_allocates_nothing() {
    let log = AccessLog::new(big()).expect("разбор");
    let (sum, allocs) = allocations(|| log.entries().map(|e| e.status as u64).sum::<u64>());
    assert_eq!(sum, 20_000 / 4 * (200 + 300 + 400 + 500));
    assert_eq!(allocs, 0, "обход журнала без экранирования не должен выделять память: поля это куски буфера, агент Cow::Borrowed");
    let (first, allocs) = allocations(|| log.with_status(500, 599).next().map(|e| e.status));
    assert_eq!((first, allocs), (Some(500), 0), "первая запись по фильтру находится лениво, без промежуточных коллекций");
}
