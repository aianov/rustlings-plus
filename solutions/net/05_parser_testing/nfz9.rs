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

fn panic_message(payload: Box<dyn Any + Send>) -> String {
    match payload.downcast::<String>() {
        Ok(s) => *s,
        Err(p) => p.downcast_ref::<&str>().map(|s| s.to_string()).unwrap_or_else(|| "паника без сообщения".to_string()),
    }
}

fn fuzz(target: fn(&[u8]), corpus: &[Vec<u8>], seed: u64, iterations: u64) -> Option<(Vec<u8>, String)> {
    for iteration in 0..iterations {
        let mut rng = Rng::new(seed.wrapping_add(iteration));
        let base = rng.pick(corpus).clone();
        let input = mutate(&mut rng, &base, corpus);
        if let Err(payload) = catch_unwind(AssertUnwindSafe(|| target(&input))) {
            return Some((input, panic_message(payload)));
        }
    }
    None
}

#[derive(Debug, PartialEq)]
struct Message {
    name: String,
    body: Vec<u8>,
    rest: usize,
}

fn decode(buf: &[u8]) -> Result<Message, String> {
    let colon = buf.iter().position(|b| *b == b':').ok_or("нет ':'")?;
    let mut len: usize = 0;
    for &b in &buf[..colon] {
        let digit = (b as char).to_digit(16).ok_or("длина не в hex")?;
        len = len.checked_mul(16).and_then(|l| l.checked_add(digit as usize)).ok_or("длина слишком велика")?;
    }
    let newline = buf[colon..].iter().position(|b| *b == b'\n').ok_or("нет конца заголовка")? + colon;
    let name = std::str::from_utf8(&buf[colon + 1..newline]).map_err(|_| "имя не UTF-8")?;
    let short: String = name.chars().take(4).collect();
    let start = newline + 1;
    let end = start.checked_add(len).filter(|end| *end <= buf.len()).ok_or("тело короче заявленной длины")?;
    Ok(Message { name: short, body: buf[start..end].to_vec(), rest: buf.len() - end })
}

fn main() {
    println!("{:?}", decode(b"3:ping\nabc"));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(data: &[u8]) {
        let _ = decode(data);
    }

    #[test]
    fn normal_messages() {
        assert_eq!(decode(b"3:ping\nabcXYZ"), Ok(Message { name: "ping".into(), body: b"abc".to_vec(), rest: 3 }));
        assert_eq!(decode(b"0:\n"), Ok(Message { name: String::new(), body: vec![], rest: 0 }));
        assert_eq!(decode(b"A:long-name\n0123456789").map(|m| (m.name, m.body.len())), Ok(("long".to_string(), 10)));
        assert!(decode(b"zz:x\nab").is_err());
        assert!(decode(b"3:x").is_err());
    }

    #[test]
    fn fuzzer_finds_nothing() {
        let corpus = vec![b"3:ping\nabc".to_vec(), b"0:\n".to_vec(), "2:имя\nxy".as_bytes().to_vec(), b"A:name\n0123456789tail".to_vec()];
        if let Some((input, message)) = fuzz(target, &corpus, 20240101, 150_000) {
            panic!("фаззер уронил декодер: {message}; вход {input:?} ({:?})", String::from_utf8_lossy(&input));
        }
        assert!(decode(b"FFFFFFFFFFFFFFFFFF:x\nabc").is_err(), "найденные фаззером границы: огромная длина должна быть Err");
        assert!(decode(b"FFFFFFFFFFFFFFFF:x\n").is_err(), "найденные фаззером границы: длина у предела usize");
        assert_eq!(decode("1:aéé\nX".as_bytes()).expect("имя из трёх символов").name, "aéé", "найденные фаззером границы: многобайтовое имя");
        assert_eq!(decode("0:ёжикам\n".as_bytes()).unwrap().name, "ёжик");
        assert!(decode(b"5:n\nab").is_err(), "найденные фаззером границы: тела меньше заявленного");
        assert!(decode(b"1:n\n").is_err());
        assert_eq!(decode(b"2:n\nab").map(|m| m.rest), Ok(0));
        assert!(decode(b"\n:").is_err());
        let _ = decode(b":");
    }
}
