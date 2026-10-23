// Список чисел можно записать двумя способами. Старый: каждое число отдельным полем с тем же номером. Новый, он же
// стандартный в proto3: все числа подряд внутри одного поля со способом Len («упакованный» список). Кто-то отправит нам
// первый вид, кто-то второй, а одно сообщение вообще может содержать оба, потому что его собирали по частям.
// TODO: Допиши decode_scores (поле 4, repeated int32, принимает оба вида вперемешку, порядок сохраняется) и encode_scores_packed
// (упакованный вид; пустой список не занимает ни байта).

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

fn put_varint(mut v: u64, out: &mut Vec<u8>) {
    loop {
        let low = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(low);
            return;
        }
        out.push(low | 0x80);
    }
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

const SCORES: u32 = 4;

fn decode_scores(buf: &[u8]) -> Result<Vec<i32>, WireError> {
    let _ = buf;
    todo!()
}

fn encode_scores_packed(scores: &[i32]) -> Vec<u8> {
    let _ = scores;
    todo!()
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_form() {
        assert_eq!(decode_scores(&[0x22, 0x03, 0x01, 0x02, 0x03]), Ok(vec![1, 2, 3]));
        assert_eq!(encode_scores_packed(&[1, 2, 3]), vec![0x22, 0x03, 0x01, 0x02, 0x03]);
        assert_eq!(encode_scores_packed(&[300]), vec![0x22, 0x02, 0xac, 0x02]);
    }

    #[test]
    fn unpacked_form() {
        assert_eq!(decode_scores(&[0x20, 0x01, 0x20, 0x02, 0x20, 0xac, 0x02]), Ok(vec![1, 2, 300]));
    }

    #[test]
    fn both_forms_in_one_message_keep_the_order() {
        let bytes = [0x20, 0x09, 0x22, 0x02, 0x01, 0x02, 0x20, 0x08, 0x22, 0x01, 0x07];
        assert_eq!(decode_scores(&bytes), Ok(vec![9, 1, 2, 8, 7]), "значения идут в порядке появления в потоке, независимо от вида записи");
    }

    #[test]
    fn negatives_take_ten_bytes_each() {
        let packed = encode_scores_packed(&[-1, 5]);
        assert_eq!(packed.len(), 2 + 10 + 1);
        assert_eq!(decode_scores(&packed), Ok(vec![-1, 5]));
    }

    #[test]
    fn empty_list_costs_nothing_and_empty_packed_field_is_fine() {
        assert_eq!(encode_scores_packed(&[]), Vec::<u8>::new(), "пустой repeated на проводе отсутствует");
        assert_eq!(decode_scores(&[]), Ok(vec![]));
        assert_eq!(decode_scores(&[0x22, 0x00]), Ok(vec![]), "пустой упакованный список тоже допустим");
    }

    #[test]
    fn other_fields_are_ignored_and_roundtrip_works() {
        let mut bytes = vec![0x08, 0x05, 0x12, 0x01, b'z'];
        bytes.extend_from_slice(&encode_scores_packed(&[10, -20, 30_000]));
        bytes.extend_from_slice(&[0x28, 0x01]);
        assert_eq!(decode_scores(&bytes), Ok(vec![10, -20, 30_000]));
    }

    #[test]
    fn broken_packed_data_is_an_error() {
        assert_eq!(decode_scores(&[0x22, 0x02, 0x01, 0x80]), Err(WireError::BadVarint), "число внутри упакованного поля оборвалось");
        assert_eq!(decode_scores(&[0x22, 0x05, 0x01]), Err(WireError::Truncated), "длина больше данных");
        assert_eq!(decode_scores(&[0x25, 1, 2, 3, 4]), Err(WireError::WrongType { field: 4 }), "поле 4 пришло фиксированной шириной");
    }
}
