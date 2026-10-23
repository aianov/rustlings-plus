// Когда нет готового ответа «как правильно», можно сравнить быструю реализацию с медленной и очевидно верной на
// одних и тех же входах. Расхождение это баг в одной из них. Паника тоже результат: если эталон вернул
// ошибку, а быстрая версия упала, это расхождение.
// TODO: Допиши differential: для cases входов из gen_input (seed + номер случая) прогони быструю и эталонную
// функцию (панику лови), на первом расхождении сожми вход готовой shrink так, чтобы расхождение осталось,
// и верни Mismatch с исходами на сжатом входе.

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

use std::panic::{AssertUnwindSafe, catch_unwind};

#[derive(Debug, Clone, PartialEq)]
enum Outcome {
    Ok(String),
    Err,
    Panic,
}

#[derive(Debug)]
struct Mismatch {
    input: Vec<u8>,
    fast: Outcome,
    reference: Outcome,
}

type Decoder = fn(&[u8]) -> Result<String, ()>;

fn gen_input(rng: &mut Rng) -> Vec<u8> {
    const ALPHABET: &[u8] = b"%0123456789abcdefABCDEFg+ xyz";
    (0..rng.below(12)).map(|_| *rng.pick(ALPHABET)).collect()
}

fn shrink(input: &[u8], still_fails: impl Fn(&[u8]) -> bool) -> Vec<u8> {
    let mut cur = input.to_vec();
    if !still_fails(&cur) {
        return cur;
    }
    let mut chunk = (cur.len() / 2).max(1);
    loop {
        let mut progress = false;
        let mut start = 0;
        while start < cur.len() {
            let end = (start + chunk).min(cur.len());
            let mut candidate = cur[..start].to_vec();
            candidate.extend_from_slice(&cur[end..]);
            if still_fails(&candidate) {
                cur = candidate;
                progress = true;
            } else {
                start += chunk;
            }
        }
        if chunk > 1 {
            chunk = (chunk / 2).max(1);
        } else if !progress {
            break;
        }
    }
    cur
}

fn outcome(decoder: Decoder, input: &[u8]) -> Outcome {
    todo!()
}

fn differential(fast: Decoder, reference: Decoder, seed: u64, cases: u64) -> Option<Mismatch> {
    todo!()
}

fn reference_decode(s: &[u8]) -> Result<String, ()> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < s.len() {
        if s[i] == b'%' {
            let hex = s.get(i + 1..i + 3).ok_or(())?;
            if !hex.iter().all(u8::is_ascii_hexdigit) {
                return Err(());
            }
            let text = std::str::from_utf8(hex).map_err(|_| ())?;
            out.push(u8::from_str_radix(text, 16).map_err(|_| ())?);
            i += 3;
        } else {
            out.push(s[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| ())
}

fn main() {
    println!("{:?}", outcome(reference_decode, b"%41"));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digit(b: u8, lowercase_bug: bool) -> Option<u8> {
        match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'A'..=b'F' => Some(b - b'A' + 10),
            b'a'..=b'f' => Some(b - b'a' + if lowercase_bug { 9 } else { 10 }),
            _ => None,
        }
    }

    fn fast(s: &[u8], lowercase_bug: bool, panic_bug: bool) -> Result<String, ()> {
        let mut out = Vec::with_capacity(s.len());
        let mut i = 0;
        while i < s.len() {
            if s[i] == b'%' {
                if panic_bug && i + 1 == s.len() {
                    let _ = s[i + 1];
                }
                let (hi, lo) = match (s.get(i + 1), s.get(i + 2)) {
                    (Some(h), Some(l)) => (digit(*h, lowercase_bug).ok_or(())?, digit(*l, lowercase_bug).ok_or(())?),
                    _ => return Err(()),
                };
                out.push(hi << 4 | lo);
                i += 3;
            } else {
                out.push(s[i]);
                i += 1;
            }
        }
        String::from_utf8(out).map_err(|_| ())
    }

    fn correct(s: &[u8]) -> Result<String, ()> {
        fast(s, false, false)
    }

    fn lowercase_bug(s: &[u8]) -> Result<String, ()> {
        fast(s, true, false)
    }

    fn panics_on_trailing_percent(s: &[u8]) -> Result<String, ()> {
        fast(s, false, true)
    }

    #[test]
    fn identical_implementations_never_differ() {
        assert!(differential(correct, reference_decode, 1, 5000).is_none(), "быстрая версия без багов совпадает с эталоном");
    }

    #[test]
    fn finds_a_wrong_value_and_shrinks_it() {
        let m = differential(lowercase_bug, reference_decode, 1, 5000).expect("строчные hex-цифры дают другое число: расхождение должно найтись");
        assert!(m.input.len() <= 3, "вход должен быть сжат до «%Xa»-подобного, получено {} байт: {:?}", m.input.len(), String::from_utf8_lossy(&m.input));
        assert_ne!(m.fast, m.reference);
        assert_eq!(m.fast, outcome(lowercase_bug, &m.input), "исходы в отчёте должны соответствовать сжатому входу");
        assert!(matches!(m.reference, Outcome::Ok(_)), "эталон на этом входе возвращает значение: {:?}", m.reference);
    }

    #[test]
    fn a_panic_is_a_difference() {
        let m = differential(panics_on_trailing_percent, reference_decode, 1, 5000).expect("быстрая версия падает там, где эталон отвечает ошибкой");
        assert_eq!(m.fast, Outcome::Panic);
        assert_eq!(m.reference, Outcome::Err);
        assert!(m.input.len() <= 2, "минимальный вход это одинокий '%' или близко к нему, а получено {:?}", String::from_utf8_lossy(&m.input));
    }

    #[test]
    fn outcome_distinguishes_all_three() {
        assert_eq!(outcome(reference_decode, b"%41"), Outcome::Ok("A".into()));
        assert_eq!(outcome(reference_decode, b"%zz"), Outcome::Err);
        assert_eq!(outcome(panics_on_trailing_percent, b"%"), Outcome::Panic);
    }
}
