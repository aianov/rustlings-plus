// Два классификатора для сетевого кода. Первый смотрит на байт из входящего потока и решает, что
// это: цифра (и какая), буква, пробельный символ, управляющий байт. Второй раскладывает код ответа
// сервера по группам, причём «слишком много запросов» и «шлюз упал» требуют особого отношения.
// В тестах несколько ловушек на порядок: частные случаи должны побеждать общие диапазоны.
// TODO: Реализуй обе функции одним `match` на каждую. Условие «код из списка» и «байт из диапазона
// с именем» в образцах записываются без цепочек `if`.

#[derive(Debug, PartialEq)]
pub enum ByteClass {
    Digit(u8),
    Upper,
    Lower,
    Space,
    Control,
    Other,
}

pub fn classify(byte: u8) -> ByteClass {
    todo!("цифра хранит свое значение 0..=9; \\t \\n \\r и пробел это Space; прочие 0x00..=0x1F и 0x7F это Control")
}

#[derive(Debug, PartialEq)]
pub enum Reply {
    Info,
    Success,
    NoContent,
    Redirect { code: u16, permanent: bool },
    RateLimited,
    ClientError(u16),
    ServerError { code: u16, retry: bool },
    Invalid(u16),
}

pub fn reply_kind(code: u16) -> Reply {
    todo!("1xx, 2xx, 3xx, 4xx, 5xx и всё остальное; особые: 204 и 205, 301 и 308, 429, 502 503 504")
}

fn main() {
    println!("{:?} {:?}", classify(b'7'), reply_kind(503));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digits_carry_their_value() {
        for d in 0..=9u8 {
            assert_eq!(classify(b'0' + d), ByteClass::Digit(d), "символ цифры {d} должен нести значение {d}, а не код ASCII");
        }
    }

    #[test]
    fn letters_and_neighbours() {
        assert_eq!(classify(b'A'), ByteClass::Upper);
        assert_eq!(classify(b'Z'), ByteClass::Upper);
        assert_eq!(classify(b'a'), ByteClass::Lower);
        assert_eq!(classify(b'z'), ByteClass::Lower);
        assert_eq!(classify(b'@'), ByteClass::Other, "'@' стоит сразу перед 'A'");
        assert_eq!(classify(b'['), ByteClass::Other, "символ открывающей квадратной скобки стоит сразу после Z");
        assert_eq!(classify(b'`'), ByteClass::Other, "'`' стоит сразу перед 'a'");
        assert_eq!(classify(b'{'), ByteClass::Other, "символ фигурной скобки стоит сразу после z");
        assert_eq!(classify(b'/'), ByteClass::Other, "'/' стоит сразу перед '0'");
        assert_eq!(classify(b':'), ByteClass::Other, "':' стоит сразу после '9'");
    }

    #[test]
    fn whitespace_wins_over_control() {
        for b in [b' ', b'\t', b'\n', b'\r'] {
            assert_eq!(classify(b), ByteClass::Space, "байт {b:#04x} это пробельный символ, а не управляющий");
        }
        assert_eq!(classify(0x00), ByteClass::Control);
        assert_eq!(classify(0x1F), ByteClass::Control);
        assert_eq!(classify(0x7F), ByteClass::Control, "DEL тоже управляющий");
        assert_eq!(classify(0x0B), ByteClass::Control, "вертикальная табуляция не в нашем списке пробелов");
        assert_eq!(classify(0x80), ByteClass::Other, "байты выше ASCII не управляющие");
        assert_eq!(classify(0xFF), ByteClass::Other);
    }

    #[test]
    fn informational_and_success() {
        assert_eq!(reply_kind(100), Reply::Info);
        assert_eq!(reply_kind(199), Reply::Info);
        assert_eq!(reply_kind(200), Reply::Success);
        assert_eq!(reply_kind(201), Reply::Success);
        assert_eq!(reply_kind(299), Reply::Success);
        assert_eq!(reply_kind(204), Reply::NoContent, "204 особый случай внутри 2xx");
        assert_eq!(reply_kind(205), Reply::NoContent, "205 особый случай внутри 2xx");
    }

    #[test]
    fn redirects() {
        assert_eq!(reply_kind(301), Reply::Redirect { code: 301, permanent: true });
        assert_eq!(reply_kind(308), Reply::Redirect { code: 308, permanent: true });
        assert_eq!(reply_kind(302), Reply::Redirect { code: 302, permanent: false });
        assert_eq!(reply_kind(307), Reply::Redirect { code: 307, permanent: false });
        assert_eq!(reply_kind(399), Reply::Redirect { code: 399, permanent: false }, "любой другой 3xx временный");
    }

    #[test]
    fn client_errors_and_rate_limit() {
        assert_eq!(reply_kind(400), Reply::ClientError(400));
        assert_eq!(reply_kind(404), Reply::ClientError(404));
        assert_eq!(reply_kind(499), Reply::ClientError(499));
        assert_eq!(reply_kind(429), Reply::RateLimited, "429 не обычная клиентская ошибка: частный случай идёт раньше диапазона");
    }

    #[test]
    fn server_errors_with_retry() {
        assert_eq!(reply_kind(500), Reply::ServerError { code: 500, retry: false });
        assert_eq!(reply_kind(501), Reply::ServerError { code: 501, retry: false });
        assert_eq!(reply_kind(502), Reply::ServerError { code: 502, retry: true });
        assert_eq!(reply_kind(503), Reply::ServerError { code: 503, retry: true });
        assert_eq!(reply_kind(504), Reply::ServerError { code: 504, retry: true });
        assert_eq!(reply_kind(505), Reply::ServerError { code: 505, retry: false });
        assert_eq!(reply_kind(599), Reply::ServerError { code: 599, retry: false });
    }

    #[test]
    fn written_as_match_with_ranges() {
        let prod = include_str!("mtch3.rs").split("#[cfg(test)]").next().expect("файл не пуст");
        assert!(prod.matches("match ").count() >= 2, "обе функции должны быть записаны через `match`");
        assert!(prod.contains("..=") && prod.contains(" @ "), "нужны диапазоны `a..=b` и привязка через `@`");
    }

    #[test]
    fn nonsense_codes() {
        assert_eq!(reply_kind(0), Reply::Invalid(0));
        assert_eq!(reply_kind(99), Reply::Invalid(99));
        assert_eq!(reply_kind(600), Reply::Invalid(600));
        assert_eq!(reply_kind(u16::MAX), Reply::Invalid(u16::MAX));
    }
}
