use minifuzz::*;
use std::cell::Cell;
use std::time::{Duration, Instant};

fn cfg(seed: u64, iterations: u64) -> Config {
    Config { seed, max_iterations: iterations, max_time: Duration::from_secs(20) }
}

fn corpus() -> Vec<Vec<u8>> {
    vec![b"GET / HTTP/1.1\r\nHost: x\r\n\r\n".to_vec(), b"POST /a HTTP/1.1\r\nContent-Length: 2\r\n\r\nhi".to_vec()]
}

fn planted(data: &[u8]) {
    if data.windows(2).any(|w| w == [0xFF, b'\n']) {
        panic!("planted: 0xFF перед переводом строки");
    }
}

#[test]
fn rng_matches_splitmix64() {
    let mut rng = Rng::new(0);
    assert_eq!(rng.next_u64(), 0xE220_A839_7B1D_CDAF);
    assert_eq!(rng.next_u64(), 0x6E78_9E6A_A1B9_65F4);
    assert_eq!(rng.next_u64(), 0x06C4_5D18_8009_454F);
    let mut a = Rng::new(9);
    let mut b = Rng::new(9);
    assert!((0..50).all(|_| a.next_u64() == b.next_u64()), "один seed, одна последовательность");
}

#[test]
fn rng_helpers_respect_bounds() {
    let mut r = Rng::new(4);
    assert!((0..1000).all(|_| r.below(7) < 7));
    assert!((0..1000).all(|_| (5..9).contains(&r.range(5, 9))));
    assert!((0..100).all(|_| !r.chance(0)));
    assert!((0..100).all(|_| r.chance(100)));
    let items = [1, 2, 3];
    assert!((0..100).all(|_| items.contains(r.pick(&items))));
}

#[test]
fn mutate_is_deterministic_and_changes_things() {
    let input = b"GET /a HTTP/1.1\r\n\r\n";
    let c = corpus();
    let mut changed = 0;
    for s in 0..1000 {
        let a = mutate(&mut Rng::new(s), input, &c);
        assert_eq!(a, mutate(&mut Rng::new(s), input, &c), "seed {s}");
        if a != input {
            changed += 1;
        }
    }
    assert!(changed >= 850, "изменено {changed} из 1000");
    for s in 0..200 {
        let _ = mutate(&mut Rng::new(s), b"", &[]);
    }
}

#[test]
fn shrink_finds_the_minimum_with_few_calls() {
    let mut input: Vec<u8> = (0..100_000u32).map(|i| b'0' + (i % 10) as u8).collect();
    input[70_000] = b'X';
    let calls = Cell::new(0u32);
    let result = shrink(&input, |d| {
        calls.set(calls.get() + 1);
        d.contains(&b'X')
    });
    assert_eq!(result, b"X");
    assert!(calls.get() < 1500, "проверок {}: куски надо убирать большими порциями", calls.get());
    let even = shrink(&input[..600], |d| d.len() % 2 == 0 && d.contains(&b'7'));
    assert_eq!(even.len(), 2, "чётная длина: получено {} байт", even.len());
    assert_eq!(shrink(b"fine", |_| false), b"fine");
}

#[test]
fn fuzz_finds_and_shrinks_the_planted_bug() {
    let c = corpus();
    let crash = fuzz(&cfg(1, 500_000), &c, planted).expect("баг должен найтись");
    assert_eq!(crash.input, [0xFF, b'\n'], "сжатый вход должен быть минимальным: {:?}", crash.input);
    assert!(crash.original.len() >= crash.input.len());
    assert!(std::panic::catch_unwind(|| planted(&crash.original)).is_err(), "исходный вход обязан воспроизводить панику");
    assert!(crash.message.contains("planted"), "сообщение потеряно: {:?}", crash.message);
}

#[test]
fn crash_is_replayable_and_deterministic() {
    let c = corpus();
    let a = fuzz(&cfg(5, 500_000), &c, planted).unwrap();
    let b = fuzz(&cfg(5, 500_000), &c, planted).unwrap();
    assert_eq!((a.iteration, &a.original), (b.iteration, &b.original), "тот же seed, тот же сбой");
    assert_eq!(input_for(&c, 5, a.iteration), a.original, "по seed и номеру итерации вход восстанавливается");
}

#[test]
fn iteration_budget_is_exact() {
    let calls = Cell::new(0u64);
    let res = fuzz(&cfg(1, 777), &corpus(), |_| calls.set(calls.get() + 1));
    assert!(res.is_none());
    assert_eq!(calls.get(), 777, "цель вызвана {} раз вместо 777", calls.get());
}

#[test]
fn time_budget_stops_the_run() {
    let calls = Cell::new(0u64);
    let config = Config { seed: 1, max_iterations: u64::MAX, max_time: Duration::from_millis(150) };
    let started = Instant::now();
    let res = fuzz(&config, &corpus(), |_| {
        calls.set(calls.get() + 1);
        std::thread::sleep(Duration::from_millis(5));
    });
    assert!(res.is_none());
    assert!(started.elapsed() < Duration::from_millis(600), "прогон длился {:?} при бюджете 150 мс", started.elapsed());
    assert!((5..=40).contains(&calls.get()), "за 150 мс при паузе в 5 мс ожидалось 5..40 вызовов, а было {}", calls.get());
}

#[test]
fn string_and_str_panic_messages_survive() {
    let c = vec![vec![b'a'; 30]];
    let long = fuzz(&cfg(1, 100_000), &c, |d| {
        if d.len() > 40 {
            panic!("длина {}", d.len());
        }
    })
    .expect("вставки должны вытянуть длину");
    assert!(long.message.starts_with("длина "), "{:?}", long.message);
    assert_eq!(long.input.len(), 41, "сжатие до границы: 41 байт");
    assert!(fuzz(&cfg(1, 50), &[], |_| {}).is_none(), "пустой корпус допустим");
}

#[test]
fn property_runner_reports_first_failing_seed() {
    let calls = Cell::new(0);
    assert!(run_property(80, 3, |_| { calls.set(calls.get() + 1); Ok(()) }).is_ok());
    assert_eq!(calls.get(), 80);
    let prop = |rng: &mut Rng| if rng.below(10) == 4 { Err("нашли".to_string()) } else { Ok(()) };
    let first = (500..).find(|s| prop(&mut Rng::new(*s)).is_err()).unwrap();
    let fail = run_property(10_000, 500, prop).unwrap_err();
    assert_eq!(fail.seed, first);
    assert_eq!(fail.message, "нашли");
}

fn lines(chunks: &[&[u8]]) -> Vec<String> {
    let mut buf = Vec::new();
    let mut out = Vec::new();
    for chunk in chunks {
        for &b in chunk.iter() {
            if b == b'\n' {
                let mut line = std::mem::take(&mut buf);
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                out.push(String::from_utf8_lossy(&line).into_owned());
            } else {
                buf.push(b);
            }
        }
    }
    out
}

fn lines_buggy(chunks: &[&[u8]]) -> Vec<String> {
    let trimmed: Vec<&[u8]> = chunks.iter().map(|c| c.strip_suffix(b"\r").unwrap_or(c)).collect();
    lines(&trimmed)
}

#[test]
fn split_checker_passes_and_fails_correctly() {
    assert!(check_splits(b"a\rb\r\nc\n", lines).is_ok());
    assert!(check_splits(b"", lines).is_ok());
    let fail = check_splits(b"a\rb\r\n", lines_buggy).expect_err("потеря '\\r' на границе должна ловиться");
    assert_eq!(fail.cuts.len(), 1, "сначала разрезы на два куска: {:?}", fail.cuts);
    assert_ne!(fail.whole, fail.split);
    assert_eq!(fail.whole, vec!["a\rb".to_string()]);
}
