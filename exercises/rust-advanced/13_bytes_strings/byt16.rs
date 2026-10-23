// Кодировщик пишет длину текста в два байта и число получателей в один. Оператор `as` не проверяет, помещается
// ли число: `70_000 as u16` молча даёт 4464, и в сеть уходит заголовок, который врёт о размере. Приёмник
// прочитает 4464 байта, а остальное примет за начало следующего сообщения. Так ломаются протоколы.
// TODO: Сделай так, чтобы слишком большие значения были ошибкой, а не обрезались. Для безопасных
// сужающих преобразований у числовых типов есть `try_from`.
#[derive(Debug, PartialEq)]
pub enum EncodeError {
    TooLong(usize),
    TooMany(usize),
}

pub fn encode_message(text: &[u8]) -> Result<Vec<u8>, EncodeError> {
    let mut out = Vec::with_capacity(2 + text.len());
    out.extend_from_slice(&(text.len() as u16).to_be_bytes());
    out.extend_from_slice(text);
    Ok(out)
}

pub fn encode_recipients(ids: &[u32]) -> Result<Vec<u8>, EncodeError> {
    let mut out = vec![ids.len() as u8];
    for id in ids {
        out.extend_from_slice(&id.to_be_bytes());
    }
    Ok(out)
}

fn main() {
    println!("{:?}", encode_message(b"hi"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_normal_message_has_a_length_prefix() {
        assert_eq!(encode_message(b"hi"), Ok(vec![0, 2, b'h', b'i']));
        assert_eq!(encode_message(b""), Ok(vec![0, 0]));
    }

    #[test]
    fn the_largest_message_fits_exactly() {
        let text = vec![b'x'; 65535];
        let out = encode_message(&text).expect("65535 помещается в u16");
        assert_eq!(&out[..2], &[0xff, 0xff]);
        assert_eq!(out.len(), 65537);
    }

    #[test]
    fn a_too_long_message_is_an_error_not_a_lie() {
        assert_eq!(encode_message(&vec![0; 65536]), Err(EncodeError::TooLong(65536)), "65536 по модулю 65536 это ноль: тихая потеря всего текста");
        assert_eq!(encode_message(&vec![0; 70_000]), Err(EncodeError::TooLong(70_000)));
    }

    #[test]
    fn recipients_have_a_one_byte_count() {
        assert_eq!(encode_recipients(&[1, 258]), Ok(vec![2, 0, 0, 0, 1, 0, 0, 1, 2]));
        assert_eq!(encode_recipients(&[]), Ok(vec![0]));
        let many = vec![7; 255];
        assert_eq!(encode_recipients(&many).map(|v| v[0]), Ok(255));
        assert_eq!(encode_recipients(&vec![7; 256]), Err(EncodeError::TooMany(256)), "256 как u8 это ноль получателей");
    }
}
