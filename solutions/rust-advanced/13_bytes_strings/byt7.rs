#[derive(Debug, PartialEq)]
pub enum Verdict {
    Valid,
    Incomplete { valid_up_to: usize },
    Invalid { valid_up_to: usize, bad_len: usize },
}

pub fn check(bytes: &[u8]) -> Verdict {
    match std::str::from_utf8(bytes) {
        Ok(_) => Verdict::Valid,
        Err(e) => match e.error_len() {
            None => Verdict::Incomplete { valid_up_to: e.valid_up_to() },
            Some(bad_len) => Verdict::Invalid { valid_up_to: e.valid_up_to(), bad_len },
        },
    }
}

pub fn valid_prefix(bytes: &[u8]) -> &str {
    match std::str::from_utf8(bytes) {
        Ok(text) => text,
        Err(e) => std::str::from_utf8(&bytes[..e.valid_up_to()]).unwrap_or_default(),
    }
}

pub fn lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn main() {
    println!("{:?}", check("привет".as_bytes()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_good_string_is_valid() {
        assert_eq!(check("привет 👋".as_bytes()), Verdict::Valid);
        assert_eq!(check(b""), Verdict::Valid);
    }

    #[test]
    fn a_cut_character_means_wait_for_more() {
        let bytes = "aп".as_bytes();
        assert_eq!(check(&bytes[..2]), Verdict::Incomplete { valid_up_to: 1 }, "от буквы п пришёл один байт из двух");
        let emoji = "x👋".as_bytes();
        for cut in 2..emoji.len() {
            assert_eq!(check(&emoji[..cut]), Verdict::Incomplete { valid_up_to: 1 }, "эмодзи обрезан после {} байт", cut - 1);
        }
    }

    #[test]
    fn garbage_is_garbage_even_at_the_end() {
        assert_eq!(check(&[b'a', 0xff]), Verdict::Invalid { valid_up_to: 1, bad_len: 1 }, "байт 0xFF не бывает в UTF-8 вообще");
        assert_eq!(check(&[0xe2, 0x28, 0xa1]), Verdict::Invalid { valid_up_to: 0, bad_len: 1 }, "за началом трёхбайтовой буквы идёт не продолжение");
        assert_eq!(check(&[0xc0, 0xaf]), Verdict::Invalid { valid_up_to: 0, bad_len: 1 });
    }

    #[test]
    fn the_valid_prefix_stops_before_the_problem() {
        assert_eq!(valid_prefix("привет".as_bytes()), "привет");
        let mut bytes = "привет".as_bytes().to_vec();
        bytes.push(0xd0);
        assert_eq!(valid_prefix(&bytes), "привет");
        assert_eq!(valid_prefix(&[0xff, b'a']), "");
        assert_eq!(valid_prefix(b""), "");
    }

    #[test]
    fn lossy_replaces_only_the_garbage() {
        assert_eq!(lossy(b"ab\xffcd"), "ab\u{fffd}cd");
        assert_eq!(lossy("ok".as_bytes()), "ok");
        assert_eq!(lossy(&[b'a', 0xd0]), "a\u{fffd}", "обрезанный в конце символ тоже превращается в замену");
    }
}
