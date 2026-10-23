// Теперь соберём фаззер. Он берёт вход из корпуса, мутирует и скармливает цели; если цель паникует,
// панику надо поймать, запомнить вход и сообщение, и главное, суметь повторить. Для этого случайность
// каждой итерации выводится из пары (seed, номер итерации), а не из общего состояния: тогда вход
// итерации 41732 можно получить, не прогоняя предыдущие.
// TODO: Допиши input_for (вход итерации по seed и номеру) и fuzz (до iterations итераций, на первой панике
// вернуть Crash). Корпус может быть пустым: тогда базовый вход пуст.

#[derive(Debug, Clone)]
struct Rng {
    state: u64,
}

#[allow(dead_code)]
impl Rng {
    fn new(seed: u64) -> Rng {
        Rng { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: u64) -> u64 {
        assert!(n > 0, "below(0) не имеет смысла");
        let limit = u64::MAX - u64::MAX % n;
        loop {
            let x = self.next_u64();
            if x < limit {
                return x % n;
            }
        }
    }

    fn range(&mut self, lo: u64, hi: u64) -> u64 {
        lo + self.below(hi - lo)
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len() as u64) as usize]
    }
}

const INTERESTING: [u8; 8] = [0, 1, 0x7f, 0x80, 0xff, b'\r', b'\n', b' '];

fn mutate(rng: &mut Rng, input: &[u8], corpus: &[Vec<u8>]) -> Vec<u8> {
    let mut out = input.to_vec();
    for _ in 0..1 + rng.below(3) {
        let len = out.len() as u64;
        match rng.below(7) {
            0 if len > 0 => {
                let i = rng.below(len) as usize;
                out[i] ^= 1 << rng.below(8);
            }
            1 if len > 0 => {
                let i = rng.below(len) as usize;
                out[i] = rng.next_u64() as u8;
            }
            2 => {
                let i = rng.below(len + 1) as usize;
                for _ in 0..1 + rng.below(4) {
                    out.insert(i, rng.next_u64() as u8);
                }
            }
            3 if len > 0 => {
                let i = rng.below(len) as usize;
                let n = 1 + rng.below(8.min(len - i as u64)) as usize;
                out.drain(i..i + n);
            }
            4 if len > 0 => {
                let i = rng.below(len) as usize;
                let n = 1 + rng.below(8.min(len - i as u64)) as usize;
                let piece = out[i..i + n].to_vec();
                out.splice(i..i, piece);
            }
            5 if !corpus.is_empty() => {
                let other = rng.pick(corpus);
                if !other.is_empty() {
                    let a = rng.below(other.len() as u64) as usize;
                    let n = 1 + rng.below((other.len() - a) as u64) as usize;
                    let at = rng.below(len + 1) as usize;
                    out.splice(at..at, other[a..a + n].iter().copied());
                }
            }
            _ => {
                let value = *rng.pick(&INTERESTING);
                if len == 0 {
                    out.push(value);
                } else {
                    let i = rng.below(len) as usize;
                    out[i] = value;
                }
            }
        }
    }
    out
}

use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};

#[derive(Debug)]
struct Crash {
    input: Vec<u8>,
    iteration: u64,
    message: String,
}

fn panic_message(payload: Box<dyn Any + Send>) -> String {
    match payload.downcast::<String>() {
        Ok(s) => *s,
        Err(p) => p.downcast_ref::<&str>().map(|s| s.to_string()).unwrap_or_else(|| "паника без сообщения".to_string()),
    }
}

fn input_for(corpus: &[Vec<u8>], seed: u64, iteration: u64) -> Vec<u8> {
    todo!()
}

fn fuzz(target: fn(&[u8]), corpus: &[Vec<u8>], seed: u64, iterations: u64) -> Option<Crash> {
    todo!()
}

fn main() {
    fn target(data: &[u8]) {
        assert!(data.len() < 100, "слишком длинный вход");
    }
    let corpus = vec![b"hello".to_vec()];
    println!("{:?}", fuzz(target, &corpus, 1, 1000).map(|c| c.iteration));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    thread_local! {
        static CALLS: Cell<u64> = const { Cell::new(0) };
    }

    fn picky(data: &[u8]) {
        CALLS.with(|c| c.set(c.get() + 1));
        if data.windows(2).any(|w| w == [0xFF, b'\n']) {
            panic!("boom: 0xFF перед переводом строки");
        }
    }

    fn clean(data: &[u8]) {
        CALLS.with(|c| c.set(c.get() + 1));
        let _ = data.len();
    }

    fn formatted(data: &[u8]) {
        if data.len() > 40 {
            panic!("длина {}", data.len());
        }
    }

    fn corpus() -> Vec<Vec<u8>> {
        vec![b"GET / HTTP/1.1\r\nHost: x\r\n\r\n".to_vec(), b"POST /a HTTP/1.1\r\n\r\n".to_vec()]
    }

    #[test]
    fn finds_a_planted_bug() {
        let crash = fuzz(picky, &corpus(), 1, 300_000).expect("за 300000 итераций фаззер должен найти 0xFF перед переводом строки");
        assert!(crash.message.contains("boom"), "сообщение паники потерялось: {:?}", crash.message);
        assert!(catch_unwind(|| picky(&crash.input)).is_err(), "сохранённый вход не воспроизводит панику");
    }

    #[test]
    fn crash_is_replayable_from_its_iteration() {
        let c = corpus();
        let crash = fuzz(picky, &c, 7, 300_000).expect("баг должен найтись");
        assert_eq!(input_for(&c, 7, crash.iteration), crash.input, "по seed и номеру итерации вход должен восстанавливаться, не прогоняя остальные");
    }

    #[test]
    fn stops_at_the_first_crash() {
        CALLS.with(|c| c.set(0));
        let crash = fuzz(picky, &corpus(), 3, 300_000).expect("баг должен найтись");
        assert_eq!(CALLS.with(|c| c.get()), crash.iteration + 1, "после первой паники цель вызывать больше нельзя");
    }

    #[test]
    fn clean_target_runs_all_iterations() {
        CALLS.with(|c| c.set(0));
        assert!(fuzz(clean, &corpus(), 1, 1234).is_none());
        assert_eq!(CALLS.with(|c| c.get()), 1234, "цель вызвана не столько раз, сколько итераций");
    }

    #[test]
    fn same_seed_same_result() {
        let a = fuzz(picky, &corpus(), 11, 300_000).unwrap();
        let b = fuzz(picky, &corpus(), 11, 300_000).unwrap();
        assert_eq!((a.iteration, a.input), (b.iteration, b.input), "фаззер должен быть детерминированным");
    }

    #[test]
    fn formatted_panic_messages_are_captured() {
        let crash = fuzz(formatted, &[vec![b'a'; 30]], 1, 100_000).expect("вставки байтов должны вытянуть длину за 40");
        assert!(crash.message.starts_with("длина "), "сообщение с форматированием (String) потеряно: {:?}", crash.message);
    }

    #[test]
    fn empty_corpus_is_fine() {
        assert!(fuzz(clean, &[], 1, 100).is_none());
    }
}
