use std::cell::Cell;
use std::panic::{catch_unwind, resume_unwind};
use std::time::{Duration, Instant};

use minifuzz::{Config, Rng, check_splits, fuzz, input_for, mutate, run_property, shrink};

fn config(seed: u64, iterations: u64) -> Config {
    Config {
        seed,
        max_iterations: iterations,
        max_time: Duration::from_secs(20),
    }
}

fn corpus() -> Vec<Vec<u8>> {
    vec![
        b"GET / HTTP/1.1\r\nHost: x\r\n\r\n".to_vec(),
        b"POST /a HTTP/1.1\r\nContent-Length: 2\r\n\r\nhi".to_vec(),
    ]
}

fn planted(data: &[u8]) {
    if data.windows(2).any(|pair| pair == [0xFF, b'\n']) {
        resume_unwind(Box::new("planted: 0xFF перед переводом строки"));
    }
}

fn spin(busy: Duration) {
    let started = Instant::now();
    while started.elapsed() < busy {
        std::hint::spin_loop();
    }
}

#[test]
fn rng_is_splitmix64() {
    let mut rng = Rng::new(0);
    assert_eq!(
        [rng.next_u64(), rng.next_u64(), rng.next_u64()],
        [
            0xE220_A839_7B1D_CDAF,
            0x6E78_9E6A_A1B9_65F4,
            0x06C4_5D18_8009_454F
        ],
        "генератор это splitmix64: те же три числа с seed 0, что в главе"
    );
    let mut a = Rng::new(9);
    let mut b = Rng::new(9);
    assert!(
        (0..50).all(|_| a.next_u64() == b.next_u64()),
        "один seed, одна последовательность"
    );
}

#[test]
fn rng_helpers_respect_bounds() {
    let mut rng = Rng::new(4);
    assert!((0..1000).all(|_| rng.below(7) < 7), "below(n) меньше n");
    assert!(
        (0..1000).all(|_| (5..9).contains(&rng.range(5, 9))),
        "range(lo, hi) в [lo, hi)"
    );
    assert!((0..100).all(|_| !rng.chance(0)), "chance(0) никогда");
    assert!((0..100).all(|_| rng.chance(100)), "chance(100) всегда");
    let items = [1, 2, 3];
    assert!(
        (0..100).all(|_| rng.pick(&items).is_some_and(|item| items.contains(item))),
        "pick берёт элемент из среза"
    );
    assert_eq!(rng.pick::<u8>(&[]), None, "из пустого среза брать нечего");
    assert_eq!(rng.below(0), 0, "below(0) не паникует");
}

#[test]
fn mutate_is_deterministic_and_changes_things() {
    let input = b"GET /a HTTP/1.1\r\n\r\n";
    let corpus = corpus();
    let mut changed = 0;
    for seed in 0..1000 {
        let mutated = mutate(&mut Rng::new(seed), input, &corpus);
        assert_eq!(
            mutated,
            mutate(&mut Rng::new(seed), input, &corpus),
            "мутация с seed {seed} повторяется байт в байт"
        );
        if mutated != input {
            changed += 1;
        }
    }
    assert!(
        changed >= 850,
        "мутация меняет вход почти всегда: изменено {changed} из 1000"
    );
    for seed in 0..200 {
        let _ = mutate(&mut Rng::new(seed), b"", &[]);
    }
}

#[test]
fn shrink_finds_the_minimum_with_few_calls() {
    let mut input: Vec<u8> = (0..100_000u32)
        .map(|i| b"0123456789"[i as usize % 10])
        .collect();
    input[70_000] = b'X';
    let calls = Cell::new(0u32);
    let result = shrink(&input, |data| {
        calls.set(calls.get() + 1);
        data.contains(&b'X')
    });
    assert_eq!(result, b"X", "от 100 000 байт остаётся один нужный");
    assert!(
        calls.get() < 1500,
        "проверок {}: куски надо убирать большими порциями",
        calls.get()
    );
    let even = shrink(&input[..600], |data| {
        data.len() % 2 == 0 && data.contains(&b'7')
    });
    assert_eq!(even.len(), 2, "чётная длина: получено {} байт", even.len());
    assert_eq!(
        shrink(b"fine", |_| false),
        b"fine",
        "непадающий вход не трогаем"
    );
}

#[test]
fn fuzz_finds_shrinks_and_replays_the_planted_bug() {
    let corpus = corpus();
    let crash = fuzz(&config(1, 500_000), &corpus, planted).expect("подложенный баг находится");
    assert_eq!(
        crash.input,
        [0xFF, b'\n'],
        "сжатый вход минимален: {:?}",
        crash.input
    );
    assert!(
        catch_unwind(|| planted(&crash.original)).is_err(),
        "исходный вход воспроизводит панику"
    );
    assert!(
        crash.message.contains("planted"),
        "сообщение паники сохранено: {:?}",
        crash.message
    );
    assert_eq!(
        input_for(&corpus, 1, crash.iteration),
        crash.original,
        "по seed и номеру итерации вход восстанавливается"
    );
    let again = fuzz(&config(1, 500_000), &corpus, planted).expect("подложенный баг находится");
    assert_eq!(again, crash, "тот же seed, тот же сбой");
}

#[test]
fn budgets_are_respected() {
    let calls = Cell::new(0u64);
    assert!(fuzz(&config(1, 777), &corpus(), |_| calls.set(calls.get() + 1)).is_none());
    assert_eq!(
        calls.get(),
        777,
        "цель вызвана {} раз вместо 777",
        calls.get()
    );
    let calls = Cell::new(0u64);
    let limited = Config {
        seed: 1,
        max_iterations: u64::MAX,
        max_time: Duration::from_millis(150),
    };
    let started = Instant::now();
    let result = fuzz(&limited, &corpus(), |_| {
        calls.set(calls.get() + 1);
        spin(Duration::from_millis(5));
    });
    assert!(result.is_none());
    assert!(
        started.elapsed() < Duration::from_millis(600),
        "прогон длился {:?} при бюджете 150 мс",
        started.elapsed()
    );
    assert!(
        calls.get() <= 40,
        "за 150 мс по 5 мс на вход больше 40 вызовов не выйдет: {}",
        calls.get()
    );
}

#[test]
fn property_runner_reports_the_first_failing_seed() {
    let calls = Cell::new(0);
    let fine = run_property(80, 3, |_| {
        calls.set(calls.get() + 1);
        Ok(())
    });
    assert!(fine.is_ok() && calls.get() == 80, "80 случаев, 80 вызовов");
    let property = |rng: &mut Rng| {
        if rng.below(10) == 4 {
            Err(String::from("нашли"))
        } else {
            Ok(())
        }
    };
    let first = (500..)
        .find(|seed| property(&mut Rng::new(*seed)).is_err())
        .unwrap_or_default();
    let failure = run_property(10_000, 500, property).expect_err("свойство где-то нарушается");
    assert_eq!(
        (failure.seed, failure.message.as_str()),
        (first, "нашли"),
        "отчёт называет первый падающий seed: по нему случай повторяется"
    );
}

fn lines(chunks: &[&[u8]]) -> Vec<String> {
    let mut buf = Vec::new();
    let mut out = Vec::new();
    for chunk in chunks {
        for &byte in chunk.iter() {
            if byte == b'\n' {
                let mut line = std::mem::take(&mut buf);
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                out.push(String::from_utf8_lossy(&line).into_owned());
            } else {
                buf.push(byte);
            }
        }
    }
    out
}

fn lines_buggy(chunks: &[&[u8]]) -> Vec<String> {
    let trimmed: Vec<&[u8]> = chunks
        .iter()
        .map(|chunk| chunk.strip_suffix(b"\r").unwrap_or(chunk))
        .collect();
    lines(&trimmed)
}

#[test]
fn split_checker_catches_state_lost_at_a_boundary() {
    assert!(
        check_splits(b"a\rb\r\nc\n", lines).is_ok(),
        "честный разборщик проходит"
    );
    assert!(check_splits(b"", lines).is_ok(), "пустой вход тоже");
    let failure =
        check_splits(b"a\rb\r\n", lines_buggy).expect_err("потеря '\\r' на границе ловится");
    assert_eq!(
        failure.cuts.len(),
        1,
        "сначала разрезы на два куска: {:?}",
        failure.cuts
    );
    assert_eq!(failure.whole, vec![String::from("a\rb")]);
    assert_ne!(failure.whole, failure.split);
}
