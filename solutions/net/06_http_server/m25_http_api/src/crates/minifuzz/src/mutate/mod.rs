use crate::rng::Rng;

pub const INTERESTING: [u8; 8] = [0, 1, 0x7f, 0x80, 0xff, b'\r', b'\n', b' '];

pub fn mutate(rng: &mut Rng, input: &[u8], corpus: &[Vec<u8>]) -> Vec<u8> {
    let mut out = input.to_vec();
    for _ in 0..1 + rng.below(3) {
        let len = out.len();
        match rng.below(7) {
            0 if len > 0 => {
                let at = rng.index(len);
                out[at] ^= 1 << rng.below(8);
            }
            1 if len > 0 => {
                let at = rng.index(len);
                out[at] = rng.byte();
            }
            2 => {
                let at = rng.index(len + 1);
                for _ in 0..1 + rng.below(4) {
                    out.insert(at, rng.byte());
                }
            }
            3 if len > 0 => {
                let at = rng.index(len);
                let n = 1 + rng.index(8.min(len - at));
                out.drain(at..at + n);
            }
            4 if len > 0 => {
                let at = rng.index(len);
                let n = 1 + rng.index(8.min(len - at));
                let piece = out[at..at + n].to_vec();
                out.splice(at..at, piece);
            }
            5 if !corpus.is_empty() => {
                if let Some(other) = rng.pick(corpus).filter(|other| !other.is_empty()) {
                    let from = rng.index(other.len());
                    let n = 1 + rng.index(other.len() - from);
                    let at = rng.index(len + 1);
                    out.splice(at..at, other[from..from + n].iter().copied());
                }
            }
            _ => {
                let value = rng.pick(&INTERESTING).copied().unwrap_or(0);
                if len == 0 {
                    out.push(value);
                } else {
                    let at = rng.index(len);
                    out[at] = value;
                }
            }
        }
    }
    out
}
