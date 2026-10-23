// Приёмник байтов разбирает две вещи: двоичные кадры (первый байт это тип, дальше данные) и
// текстовые команды вроде `PING` и `ECHO привет`. Автор написал разбор образцами, показал
// коллегам, и тесты красные: кадры с правильной длиной определяются как «обрезанные», неизвестный
// тип перебивает известные, а `ECHO` съедает пробел или принимает `ECHOES`.
// Формат кадра:  [0x00]  Ping;  [0x01, длина, тело...]  Text, если тело ровно `длина` байт;
// [0x02, hi, lo]  Seq (u16, big-endian);  известный тип с неправильной формой  Malformed;
// любой другой первый байт  Unknown(байт);  пустой срез  Empty.
// TODO: Найди, почему образцы срабатывают не там, где задумано, и исправь. Набор образцов
// и типы можно менять как угодно, главное чтобы тесты стали зелёными.

#[derive(Debug, PartialEq)]
pub enum Frame<'a> {
    Empty,
    Ping,
    Text(&'a [u8]),
    Seq(u16),
    Malformed(u8),
    Unknown(u8),
}

pub fn decode(bytes: &[u8]) -> Frame<'_> {
    match bytes {
        [] => Frame::Empty,
        [tag, ..] => Frame::Unknown(*tag),
        [0x00] => Frame::Ping,
        [tag @ 0x00..=0x02, ..] => Frame::Malformed(*tag),
        [0x01, len, body @ ..] if body.len() == *len as usize => Frame::Text(body),
        [0x02, hi, lo] => Frame::Seq(u16::from_be_bytes([*hi, *lo])),
    }
}

#[derive(Debug, PartialEq)]
pub enum Command<'a> {
    Ping,
    Quit,
    Echo(&'a [u8]),
    Unknown,
}

pub fn command(line: &[u8]) -> Command<'_> {
    match line {
        [b'E', b'C', b'H', b'O', rest @ ..] => Command::Echo(rest),
        b"PING" => Command::Ping,
        b"QUIT" => Command::Quit,
        _ => Command::Unknown,
    }
}

fn main() {
    println!("{:?}", decode(&[0x00]));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_and_ping() {
        assert_eq!(decode(&[]), Frame::Empty);
        assert_eq!(decode(&[0x00]), Frame::Ping, "один нулевой байт это Ping");
    }

    #[test]
    fn text_frames_check_length() {
        assert_eq!(decode(&[0x01, 3, b'a', b'b', b'c']), Frame::Text(b"abc"), "длина совпала: это Text");
        assert_eq!(decode(&[0x01, 0]), Frame::Text(b""), "пустой текст допустим");
        assert_eq!(decode(&[0x01, 3, b'a', b'b']), Frame::Malformed(0x01), "тела меньше заявленного");
        assert_eq!(decode(&[0x01, 1, b'a', b'b']), Frame::Malformed(0x01), "тела больше заявленного");
        assert_eq!(decode(&[0x01]), Frame::Malformed(0x01), "даже длины нет");
    }

    #[test]
    fn seq_frames() {
        assert_eq!(decode(&[0x02, 0x01, 0x02]), Frame::Seq(258), "big-endian: 0x0102");
        assert_eq!(decode(&[0x02, 0xFF, 0xFF]), Frame::Seq(65535));
        assert_eq!(decode(&[0x02, 0x01]), Frame::Malformed(0x02), "не хватает байта");
        assert_eq!(decode(&[0x02, 1, 2, 3]), Frame::Malformed(0x02), "лишний байт");
    }

    #[test]
    fn ping_with_garbage_is_malformed() {
        assert_eq!(decode(&[0x00, 0x01]), Frame::Malformed(0x00), "за Ping не должно быть хвоста");
    }

    #[test]
    fn unknown_types() {
        assert_eq!(decode(&[0x03]), Frame::Unknown(3));
        assert_eq!(decode(&[0xFF, 1, 2, 3]), Frame::Unknown(0xFF), "хвост не важен");
    }

    #[test]
    fn text_commands() {
        assert_eq!(command(b"PING"), Command::Ping);
        assert_eq!(command(b"QUIT"), Command::Quit);
        assert_eq!(command(b"ECHO hello world"), Command::Echo(b"hello world"), "пробел после ECHO не часть текста");
        assert_eq!(command(b"ECHO "), Command::Echo(b""), "пустое эхо допустимо");
    }

    #[test]
    fn text_commands_are_exact() {
        assert_eq!(command(b"ECHOES"), Command::Unknown, "ECHOES не ECHO");
        assert_eq!(command(b"ECHO"), Command::Unknown, "без пробела и текста это не эхо");
        assert_eq!(command(b"PINGPONG"), Command::Unknown, "PING должен совпадать целиком");
        assert_eq!(command(b"ping"), Command::Unknown, "регистр важен");
        assert_eq!(command(b""), Command::Unknown);
    }
}
