
use std::cell::Cell;

pub trait Octet {
    fn value(&self) -> u8;
}

impl Octet for u8 {
    fn value(&self) -> u8 {
        *self
    }
}

pub fn ct_eq<T: Octet>(a: &[T], b: &[T]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b) {
        diff |= x.value() ^ y.value();
    }
    diff == 0
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    struct Counted<'a>(u8, &'a Cell<usize>);

    impl Octet for Counted<'_> {
        fn value(&self) -> u8 {
            self.1.set(self.1.get() + 1);
            self.0
        }
    }

    fn counted<'a>(bytes: &[u8], reads: &'a Cell<usize>) -> Vec<Counted<'a>> {
        bytes.iter().map(|b| Counted(*b, reads)).collect()
    }

    #[test]
    fn equality_on_plain_bytes() {
        assert!(ct_eq(b"secret", b"secret"), "равные");
        assert!(!ct_eq(b"secret", b"secreT"), "отличается последний");
        assert!(!ct_eq(b"secret", b"Secret"), "отличается первый");
        assert!(!ct_eq(b"secret", b"secre"), "разная длина");
        assert!(!ct_eq(b"", b"x"), "пустая против непустой");
        assert!(ct_eq::<u8>(b"", b""), "две пустые равны");
    }

    #[test]
    fn mismatch_position_does_not_change_the_work() {
        let secret = b"0123456789abcdef";
        let mut works = Vec::new();
        for position in [0usize, 7, 15] {
            let mut guess = *secret;
            guess[position] ^= 1;
            let reads = Cell::new(0);
            let (a, b) = (counted(secret, &reads), counted(&guess, &reads));
            assert!(!ct_eq(&a, &b), "различие в позиции {position}");
            works.push(reads.get());
        }
        assert_eq!(works, vec![32, 32, 32], "число прочитанных байтов не должно зависеть от места отличия: нужны все 2*16 чтений, а было {works:?}");
    }

    #[test]
    fn equal_inputs_read_everything_too() {
        let reads = Cell::new(0);
        let (a, b) = (counted(b"abcd", &reads), counted(b"abcd", &reads));
        assert!(ct_eq(&a, &b), "равные");
        assert_eq!(reads.get(), 8, "оба вектора читаются целиком");
    }
}
