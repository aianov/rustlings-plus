// Сервис профилей обновился и теперь добавляет в сообщение поля, о которых наш старый клиент не слышал. Клиент не должен
// падать: поля, которых он не знает, нужно аккуратно пропустить по их способу записи и читать дальше. Ещё одно правило,
// которое легко упустить: если одно и то же поле встретилось дважды, побеждает последнее значение.
// TODO: Допиши parse_message, которая разбирает сообщение на поля без всякой схемы, и decode_profile, которая берёт из них
// известные (1: id, 2: name, 3: age) и пропускает остальные. Ошибка в данных не повод паниковать.

#![allow(dead_code)]

#[derive(Debug, PartialEq, Clone)]
enum Value<'a> {
    Varint(u64),
    I64(u64),
    Len(&'a [u8]),
    I32(u32),
}

#[derive(Debug, PartialEq, Clone)]
struct Field<'a> {
    number: u32,
    value: Value<'a>,
}

#[derive(Debug, PartialEq)]
enum WireError {
    BadVarint,
    Truncated,
    BadKey,
    WrongType { field: u32 },
    InvalidUtf8,
}

#[derive(Debug, PartialEq, Default)]
struct Profile {
    id: u64,
    name: String,
    age: u32,
}

fn get_varint(buf: &[u8]) -> Result<(u64, usize), WireError> {
    let mut value = 0u64;
    for (i, &byte) in buf.iter().take(10).enumerate() {
        value |= u64::from(byte & 0x7f) << (7 * i);
        if byte & 0x80 == 0 {
            return Ok((value, i + 1));
        }
    }
    Err(WireError::BadVarint)
}

fn parse_message(buf: &[u8]) -> Result<Vec<Field<'_>>, WireError> {
    let mut fields = Vec::new();
    let mut pos = 0;
    while pos < buf.len() {
        let (key, used) = get_varint(&buf[pos..])?;
        pos += used;
        let number = key >> 3;
        if number == 0 || number > (1 << 29) - 1 {
            return Err(WireError::BadKey);
        }
        let rest = &buf[pos..];
        let (value, used) = match key & 7 {
            0 => {
                let (v, used) = get_varint(rest)?;
                (Value::Varint(v), used)
            }
            1 => (Value::I64(u64::from_le_bytes(rest.get(..8).ok_or(WireError::Truncated)?.try_into().unwrap())), 8),
            2 => {
                let (len, used) = get_varint(rest)?;
                let body = &rest[used..];
                if len > body.len() as u64 {
                    return Err(WireError::Truncated);
                }
                (Value::Len(&body[..len as usize]), used + len as usize)
            }
            5 => (Value::I32(u32::from_le_bytes(rest.get(..4).ok_or(WireError::Truncated)?.try_into().unwrap())), 4),
            _ => return Err(WireError::BadKey),
        };
        pos += used;
        fields.push(Field { number: number as u32, value });
    }
    Ok(fields)
}

fn decode_profile(buf: &[u8]) -> Result<Profile, WireError> {
    let mut profile = Profile::default();
    for field in parse_message(buf)? {
        match (field.number, field.value) {
            (1, Value::Varint(v)) => profile.id = v,
            (2, Value::Len(bytes)) => profile.name = std::str::from_utf8(bytes).map_err(|_| WireError::InvalidUtf8)?.to_string(),
            (3, Value::Varint(v)) => profile.age = v as u32,
            (n @ 1..=3, _) => return Err(WireError::WrongType { field: n }),
            _ => {}
        }
    }
    Ok(profile)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    const KNOWN: [u8; 12] = [0x08, 0x96, 0x01, 0x12, 0x05, b'm', b'a', b'r', b't', b'a', 0x18, 0x1e];

    #[test]
    fn known_fields_are_decoded() {
        assert_eq!(decode_profile(&KNOWN), Ok(Profile { id: 150, name: "marta".into(), age: 30 }));
        assert_eq!(decode_profile(&[]), Ok(Profile::default()), "пустое сообщение это все значения по умолчанию");
    }

    #[test]
    fn unknown_fields_of_every_kind_are_skipped() {
        let mut bytes = vec![0x78, 0x2a];
        bytes.extend_from_slice(&[0x49, 1, 2, 3, 4, 5, 6, 7, 8]);
        bytes.extend_from_slice(&KNOWN[..3]);
        bytes.extend_from_slice(&[0x82, 0x01, 0x03, b'x', b'y', b'z']);
        bytes.extend_from_slice(&KNOWN[3..10]);
        bytes.extend_from_slice(&[0xbd, 0x02, 9, 9, 9, 9]);
        bytes.extend_from_slice(&KNOWN[10..]);
        assert_eq!(decode_profile(&bytes), Ok(Profile { id: 150, name: "marta".into(), age: 30 }), "поля 15, 9, 16 и 39 всех четырёх видов должны быть пропущены");
        let parsed = parse_message(&bytes).unwrap();
        assert_eq!(parsed.len(), 7);
        assert_eq!(parsed[1], Field { number: 9, value: Value::I64(0x0807_0605_0403_0201) });
        assert_eq!(parsed[3], Field { number: 16, value: Value::Len(b"xyz") });
    }

    #[test]
    fn last_value_wins() {
        let bytes = [0x08, 0x01, 0x08, 0x02, 0x12, 0x01, b'a', 0x12, 0x01, b'b', 0x08, 0x07];
        assert_eq!(decode_profile(&bytes), Ok(Profile { id: 7, name: "b".into(), age: 0 }));
    }

    #[test]
    fn wrong_wire_type_for_a_known_field_is_an_error() {
        assert_eq!(decode_profile(&[0x0a, 0x01, b'x']), Err(WireError::WrongType { field: 1 }), "id пришёл как строка");
        assert_eq!(decode_profile(&[0x10, 0x05]), Err(WireError::WrongType { field: 2 }), "name пришло числом");
        assert_eq!(decode_profile(&[0x12, 0x01, 0xff]), Err(WireError::InvalidUtf8));
    }

    #[test]
    fn cuts_inside_a_field_are_errors_not_panics() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&KNOWN);
        bytes.extend_from_slice(&[0x49, 1, 2, 3, 4, 5, 6, 7, 8, 0x25, 1, 2, 3, 4]);
        let boundaries: Vec<usize> = vec![0, 3, 10, 12, 21, 26];
        for cut in 0..=bytes.len() {
            let result = parse_message(&bytes[..cut]);
            if boundaries.contains(&cut) {
                assert!(result.is_ok(), "обрезка на границе поля ({cut}) даёт правильное сообщение");
            } else {
                assert!(result.is_err(), "обрезка внутри поля ({cut}) должна быть ошибкой, а получилось {result:?}");
            }
        }
    }

    #[test]
    fn garbage_never_panics() {
        let mut x: u64 = 2024;
        for _ in 0..4000 {
            let mut buf = Vec::new();
            x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let len = (x >> 60) as usize + 1;
            for _ in 0..len {
                x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                buf.push((x >> 56) as u8);
            }
            let _ = parse_message(&buf);
            let _ = decode_profile(&buf);
        }
    }

    #[test]
    fn invalid_keys_are_rejected() {
        assert_eq!(parse_message(&[0x00, 0x01]), Err(WireError::BadKey), "поле номер ноль");
        assert_eq!(parse_message(&[0x0b]), Err(WireError::BadKey), "группы не поддерживаются");
        assert_eq!(parse_message(&[0x0e, 0x01]), Err(WireError::BadKey), "способ записи 6 не существует");
    }
}
