// Текст приходит из сети кусками, и граница куска не знает о границах букв. Сообщение «привет 🌍» может
// прийти как 7 байт и ещё 10: вторая буква «р» разрезана пополам. Декодер, который переваривает каждый
// кусок отдельно (`from_utf8_lossy`), портит такую букву в мусор. Нужен декодер с памятью: недочитанный
// хвост символа остаётся до следующего куска. Настоящий мусор (байты, которые никогда не бывают в UTF-8)
// заменяется на U+FFFD сразу.
// TODO: Почини `feed` и допиши `finish`: хвост, которого так и не дождались, превращается в один U+FFFD.
#[derive(Default)]
pub struct Decoder {
    pending: Vec<u8>,
}

impl Decoder {
    pub fn new() -> Decoder {
        Decoder::default()
    }

    pub fn feed(&mut self, chunk: &[u8]) -> String {
        String::from_utf8_lossy(chunk).into_owned()
    }

    pub fn finish(self) -> String {
        todo!("обрезанный хвост это один U+FFFD")
    }
}

fn main() {
    let mut d = Decoder::new();
    print!("{}", d.feed("приве".as_bytes()));
    println!("{}", d.finish());
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "привет 🌍 мир, café";

    fn decode_in_chunks(bytes: &[u8], size: usize) -> String {
        let mut d = Decoder::new();
        let mut out = String::new();
        for chunk in bytes.chunks(size) {
            out.push_str(&d.feed(chunk));
        }
        out.push_str(&d.finish());
        out
    }

    #[test]
    fn any_two_way_split_restores_the_text() {
        let bytes = TEXT.as_bytes();
        for cut in 0..=bytes.len() {
            let mut d = Decoder::new();
            let mut out = d.feed(&bytes[..cut]);
            out.push_str(&d.feed(&bytes[cut..]));
            out.push_str(&d.finish());
            assert_eq!(out, TEXT, "кусок разрезан после {cut} байт: буква на стыке испортилась");
        }
    }

    #[test]
    fn byte_by_byte_works_too() {
        for size in 1..=5 {
            assert_eq!(decode_in_chunks(TEXT.as_bytes(), size), TEXT, "куски по {size} байт");
        }
    }

    #[test]
    fn a_half_character_is_held_back() {
        let mut d = Decoder::new();
        let bytes = "п".as_bytes();
        assert_eq!(d.feed(&bytes[..1]), "", "половина буквы пока не текст");
        assert_eq!(d.feed(&bytes[1..]), "п");
        assert_eq!(d.finish(), "");
    }

    #[test]
    fn real_garbage_is_replaced_immediately() {
        let mut d = Decoder::new();
        assert_eq!(d.feed(b"ab\xffcd"), "ab\u{fffd}cd");
        assert_eq!(d.feed(b"\xc0\xaf!"), "\u{fffd}\u{fffd}!");
        assert_eq!(d.finish(), "");
    }

    #[test]
    fn a_stream_cut_off_mid_character_ends_with_one_replacement() {
        let mut d = Decoder::new();
        let bytes = "ок👋".as_bytes();
        let mut out = d.feed(&bytes[..bytes.len() - 2]);
        out.push_str(&d.finish());
        assert_eq!(out, "ок\u{fffd}", "хвост эмодзи так и не пришёл");
    }
}
