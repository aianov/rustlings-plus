#![allow(unsafe_code)]
use logq::{Aggregate, CountByLevel, Entry, Level, LevelSet, ParseError, TopTargets, entries, level_set, only, par_map_chunks, parse_line, run, summarize, summarize_parallel};
use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

mod counting {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    thread_local! {
        static ALLOCS: Cell<usize> = const { Cell::new(0) };
        static BYTES: Cell<usize> = const { Cell::new(0) };
    }

    pub struct Counting;

    fn bump(size: usize) {
        let _ = ALLOCS.try_with(|n| n.set(n.get() + 1));
        let _ = BYTES.try_with(|n| n.set(n.get() + size));
    }

    unsafe impl GlobalAlloc for Counting {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            bump(layout.size());
            unsafe { System.alloc(layout) }
        }

        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            bump(layout.size());
            unsafe { System.alloc_zeroed(layout) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            unsafe { System.dealloc(ptr, layout) }
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            bump(new_size);
            unsafe { System.realloc(ptr, layout, new_size) }
        }
    }

    pub fn measure<R>(f: impl FnOnce() -> R) -> (R, usize, usize) {
        let (a, b) = (ALLOCS.get(), BYTES.get());
        let result = f();
        (result, ALLOCS.get() - a, BYTES.get() - b)
    }
}

#[global_allocator]
static GLOBAL: counting::Counting = counting::Counting;

const SAMPLE: &str = "\
1700000001 INFO api запрос принят
1700000002 INFO db выборка
1700000003 WARN db медленный запрос
1700000004 ERROR api таймаут

1700000005 INFO api ответ отправлен
не строка журнала
1700000006 DEBUG cache промах
1700000007 INFO api запрос принят
";

fn crate_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn project_dir() -> PathBuf {
    crate_dir().parent().and_then(Path::parent).expect("крейт лежит в crates/logq").to_path_buf()
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("не удалось создать папку");
    for entry in fs::read_dir(from).expect("не удалось прочитать папку").flatten() {
        let name = entry.file_name();
        if name == "target" || name == "Cargo.lock" {
            continue;
        }
        let (src, dst) = (entry.path(), to.join(&name));
        if src.is_dir() {
            copy_tree(&src, &dst);
        } else {
            fs::copy(&src, &dst).expect("не удалось скопировать файл");
        }
    }
}

fn sandbox(name: &str) -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("exam_{name}_{}_{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
    let _ = fs::remove_dir_all(&dir);
    copy_tree(&project_dir(), &dir);
    dir
}

fn cargo(dir: &Path, args: &[&str]) -> (bool, String) {
    let output = Command::new("sh")
        .args(["-c", "exec cargo \"$@\" 2>&1", "sh"])
        .args(args)
        .env("CARGO_TARGET_DIR", dir.join("target"))
        .env("CARGO_TERM_COLOR", "never")
        .env_remove("RUSTFLAGS")
        .env_remove("CLIPPY_CONF_DIR")
        .current_dir(dir)
        .stdin(Stdio::null())
        .output()
        .expect("не удалось запустить cargo");
    (output.status.success(), String::from_utf8_lossy(&output.stdout).to_string())
}

#[test]
fn level_parses_and_prints() {
    assert_eq!("WARN".parse::<Level>(), Ok(Level::Warn));
    assert_eq!("warn".parse::<Level>(), Err(ParseError::UnknownLevel("warn".to_string())), "регистр важен");
    assert_eq!(Level::Error.to_string(), "ERROR");
    assert!(Level::Debug < Level::Info && Level::Info < Level::Warn && Level::Warn < Level::Error, "уровни упорядочены от DEBUG к ERROR");
}

#[test]
fn level_set_and_its_macro() {
    let set = level_set![Warn, Error];
    assert!(set.contains(Level::Warn) && set.contains(Level::Error));
    assert!(!set.contains(Level::Info) && !set.contains(Level::Debug));
    assert_eq!(set.len(), 2);
    let empty: LevelSet = level_set![];
    assert!(empty.is_empty() && empty == LevelSet::empty());
    assert_eq!(level_set![Info,], LevelSet::empty().with(Level::Info), "завершающая запятая допустима");
    assert_eq!(level_set![Info, Info].len(), 1, "повторы не считаются дважды");
}

#[test]
fn line_parsing_and_errors() {
    let entry = parse_line("1700000003 WARN db медленный запрос: 120 мс").expect("строка корректна");
    assert_eq!((entry.ts, entry.level, entry.target), (1_700_000_003, Level::Warn, "db"));
    assert_eq!(entry.message, "медленный запрос: 120 мс", "сообщение это всё остальное, с пробелами");
    assert_eq!(parse_line(""), Err(ParseError::MissingField("timestamp")));
    assert_eq!(parse_line("1700000003"), Err(ParseError::MissingField("level")));
    assert_eq!(parse_line("1700000003 INFO"), Err(ParseError::MissingField("target")));
    assert_eq!(parse_line("1700000003 INFO db"), Err(ParseError::MissingField("message")));
    assert_eq!(parse_line("1700000003 LOUD db текст"), Err(ParseError::UnknownLevel("LOUD".to_string())));
    assert!(matches!(parse_line("вчера INFO db текст"), Err(ParseError::BadTimestamp(_))));
}

#[test]
fn errors_are_real_errors_with_sources() {
    let bad_ts = parse_line("x INFO a b").unwrap_err();
    let as_error: &dyn Error = &bad_ts;
    assert!(as_error.source().is_some(), "у BadTimestamp источник ошибки это ParseIntError");
    assert!(bad_ts.to_string().contains("метк"), "сообщение должно говорить про метку времени: {bad_ts}");
    let unknown = parse_line("1 LOUD a b").unwrap_err();
    assert!((&unknown as &dyn Error).source().is_none());
    assert!(unknown.to_string().contains("LOUD"), "в сообщении должен быть сам уровень");
    assert!(ParseError::MissingField("target").to_string().contains("target"));
    let from_int: ParseError = "z".parse::<u64>().unwrap_err().into();
    assert!(matches!(from_int, ParseError::BadTimestamp(_)), "ParseIntError превращается в BadTimestamp через From");
}

#[test]
fn entries_skip_blank_lines_and_number_bad_ones() {
    let items: Vec<_> = entries(SAMPLE).collect();
    assert_eq!(items.iter().filter(|i| i.is_ok()).count(), 7);
    let bad: Vec<usize> = items.iter().filter_map(|i| i.as_ref().err()).map(|e| e.line).collect();
    assert_eq!(bad, vec![7], "номера строк считаются с единицы, пустые строки тоже занимают номер");
    let first = items[0].as_ref().expect("первая строка корректна");
    assert_eq!(first.message, "запрос принят");
}

#[test]
fn entries_borrow_from_the_text() {
    let text = String::from("5 INFO api привет мир\n");
    let entry = parse_line(text.lines().next().expect("строка есть")).expect("корректна");
    let range = text.as_bytes().as_ptr_range();
    assert!(range.contains(&entry.target.as_ptr()) && range.contains(&entry.message.as_ptr()), "target и message должны быть срезами исходной строки, без копий");
}

#[test]
fn iteration_is_lazy() {
    let mut text = String::with_capacity(60_000_000);
    for i in 0..1_000_000 {
        text.push_str("1700000000 INFO api сообщение номер ");
        text.push_str(&(i % 10).to_string());
        text.push('\n');
    }
    let (count, allocs, bytes) = counting::measure(|| entries(&text).take(3).count());
    assert_eq!(count, 3);
    assert!(allocs <= 2 && bytes < 4_096, "первые три записи миллионного журнала стоили {allocs} аллокаций и {bytes} байт: итератор должен быть ленивым");
}

#[test]
fn aggregates_through_the_trait() {
    let parsed = || entries(SAMPLE).filter_map(Result::ok);
    let counts = run(parsed(), CountByLevel::default());
    assert_eq!(counts, BTreeMap::from([(Level::Debug, 1), (Level::Info, 4), (Level::Warn, 1), (Level::Error, 1)]));
    let top = run(parsed(), TopTargets::new(2));
    assert_eq!(top, vec![("api".to_string(), 4), ("db".to_string(), 2)], "по убыванию числа записей");
    let ties = run(["1 INFO b x", "2 INFO a x", "3 INFO c x"].into_iter().filter_map(|l| parse_line(l).ok()), TopTargets::new(10));
    assert_eq!(ties, vec![("a".to_string(), 1), ("b".to_string(), 1), ("c".to_string(), 1)], "при равенстве по имени");
    assert!(run(parsed(), TopTargets::new(0)).is_empty());
}

#[test]
fn own_aggregate_works_with_run() {
    struct LongestMessage(usize);
    impl Aggregate for LongestMessage {
        type Out = usize;
        fn add(&mut self, entry: &Entry<'_>) {
            self.0 = self.0.max(entry.message.chars().count());
        }
        fn finish(self) -> usize {
            self.0
        }
    }
    let longest = run(entries(SAMPLE).filter_map(Result::ok), LongestMessage(0));
    assert_eq!(longest, "медленный запрос".chars().count());
}

#[test]
fn level_filter_is_an_iterator_adapter() {
    let serious: Vec<u64> = only(entries(SAMPLE).filter_map(Result::ok), level_set![Warn, Error]).map(|e| e.ts).collect();
    assert_eq!(serious, vec![1_700_000_003, 1_700_000_004]);
}

#[test]
fn aggregation_does_not_allocate_per_line() {
    let mut text = String::new();
    for i in 0..1_000 {
        text.push_str(&format!("{} INFO api запрос {i}\n", 1_700_000_000 + i));
    }
    let (counts, allocs, _) = counting::measure(|| run(entries(&text).filter_map(Result::ok), CountByLevel::default()));
    assert_eq!(counts[&Level::Info], 1_000);
    assert!(allocs <= 12, "{allocs} аллокаций на 1000 строк: разбор должен брать срезы, а не копировать строки");
}

#[test]
fn summary_counts_everything() {
    let report = summarize(SAMPLE);
    assert_eq!((report.total, report.bad_lines), (7, 1));
    assert_eq!(report.by_level[&Level::Info], 4);
    assert_eq!(summarize(""), logq::Report::default());
}

#[test]
fn chunks_are_line_aligned_and_balanced() {
    let text = "a\nb\nc\nd\ne\n";
    assert_eq!(par_map_chunks(text, 2, |chunk| chunk.to_string()), vec!["a\nb\nc\n", "d\ne\n"], "куски по числу строк, лишние строки достаются первым");
    assert_eq!(par_map_chunks("x\ny", 10, |chunk| chunk.to_string()), vec!["x\n", "y"], "строк меньше потоков: у каждой строки свой кусок");
    assert_eq!(par_map_chunks("x\ny\nz\n", 0, |c| c.len()), vec![6], "ноль потоков это один");
    assert!(par_map_chunks("", 4, |c| c.len()).is_empty());
}

#[test]
fn chunks_really_run_in_parallel() {
    let arrived = AtomicUsize::new(0);
    let met = par_map_chunks("1\n2\n3\n4\n", 4, |_| {
        arrived.fetch_add(1, Ordering::SeqCst);
        let start = std::time::Instant::now();
        while arrived.load(Ordering::SeqCst) < 4 {
            if start.elapsed() > std::time::Duration::from_secs(3) {
                return false;
            }
            std::thread::yield_now();
        }
        true
    });
    assert_eq!(met, vec![true; 4], "все четыре куска должны обрабатываться одновременно, иначе они не встретятся");
}

#[test]
fn panic_in_a_worker_reaches_the_caller() {
    let result = std::panic::catch_unwind(|| par_map_chunks("a\nb\n", 2, |chunk| if chunk.starts_with('b') { panic!("сбой в куске") } else { 1 }));
    assert!(result.is_err(), "паника рабочего потока не должна теряться");
}

#[test]
fn parallel_summary_equals_sequential() {
    let mut text = String::new();
    for i in 0..5_000u64 {
        let level = ["DEBUG", "INFO", "WARN", "ERROR"][(i % 4) as usize];
        if i % 97 == 0 {
            text.push_str("мусорная строка\n");
        } else {
            text.push_str(&format!("{} {level} svc{} сообщение {i}\n", 1_700_000_000 + i, i % 7));
        }
    }
    let expected = summarize(&text);
    for workers in [1, 2, 3, 8, 64] {
        assert_eq!(summarize_parallel(&text, workers), expected, "параллельный итог при {workers} потоках отличается от последовательного");
    }
}

#[cfg(feature = "json")]
#[test]
fn report_json_has_a_fixed_shape() {
    let report = summarize("1 INFO a x\n2 WARN a y\n3 INFO b z\nплохая\n4 ERROR a w\n5 INFO a q\n");
    assert_eq!(report.to_json(), r#"{"total":5,"badLines":1,"byLevel":{"INFO":3,"WARN":1,"ERROR":1}}"#);
    assert_eq!(logq::Report::default().to_json(), r#"{"total":0,"badLines":0,"byLevel":{}}"#);
}

#[test]
fn json_is_optional() {
    let sandbox = sandbox("features");
    fs::create_dir_all(sandbox.join("crates/logq/examples")).expect("не удалось создать папку");
    fs::write(sandbox.join("crates/logq/examples/probe_json.rs"), "fn main() {\n    println!(\"{}\", logq::Report::default().to_json());\n}\n").expect("не удалось записать файл");
    let (ok, output) = cargo(&sandbox, &["check", "-p", "logq", "--no-default-features"]);
    assert!(ok, "без feature json крейт должен собираться:\n{output}");
    let (ok, _) = cargo(&sandbox, &["check", "-p", "logq", "--no-default-features", "--example", "probe_json"]);
    assert!(!ok, "to_json не должен существовать без feature json");
    let (ok, output) = cargo(&sandbox, &["tree", "-p", "logq", "--no-default-features", "-e", "normal", "--prefix", "none", "-f", "{lib}"]);
    assert!(ok && !output.contains("serde"), "без feature json serde не должен попадать в зависимости:\n{output}");
    let (ok, output) = cargo(&sandbox, &["check", "-p", "logq", "--example", "probe_json"]);
    assert!(ok, "по умолчанию json включён:\n{output}");
}

#[test]
fn unsafe_code_is_denied() {
    let sandbox = sandbox("unsafe");
    let lib = sandbox.join("crates/logq/src/lib.rs");
    let mut source = fs::read_to_string(&lib).expect("нет src/lib.rs");
    source.push_str("\npub fn sneaky() -> u8 {\n    unsafe { std::mem::zeroed() }\n}\n");
    fs::write(&lib, source).expect("не удалось записать файл");
    let (ok, _) = cargo(&sandbox, &["check", "-p", "logq"]);
    assert!(!ok, "unsafe должен быть запрещён линтом (deny), а добавленный unsafe-блок собраться не должен");
}

#[test]
fn own_tests_and_doc_examples_exist() {
    let mut unit = 0;
    let src = crate_dir().join("src");
    let mut stack = vec![src];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).expect("нет src").flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let text = fs::read_to_string(&path).expect("не удалось прочитать файл");
                if text.contains("#[cfg(test)]") {
                    unit += text.matches("#[test]").count();
                }
            }
        }
    }
    assert!(unit >= 3, "нужны свои юнит-тесты (`#[cfg(test)]`, не меньше трёх `#[test]` в src), нашёл {unit}");
    let sandbox = sandbox("docs");
    let (ok, output) = cargo(&sandbox, &["test", "-p", "logq", "--doc"]);
    assert!(ok, "документационные тесты упали:\n{output}");
    let passed: usize = output
        .lines()
        .filter(|l| l.starts_with("test result:"))
        .filter_map(|l| l.split_whitespace().nth(3).and_then(|n| n.parse::<usize>().ok()))
        .sum();
    assert!(passed >= 2, "нужно минимум два документационных примера с проверками (у parse_line и у entries)\n{output}");
}
