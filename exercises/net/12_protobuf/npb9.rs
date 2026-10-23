// Команда доставки прислала схему своего сообщения, и теперь наш сервис обязан читать и писать ровно такие байты. Руками
// кодировать всё не нужно: крейт prost умеет это по описанию структуры. Описание состоит из derive и атрибутов
// `#[prost(тип, tag = "номер")]` у каждого поля. Номера и типы бери из схемы, форму Rust-типов ищи в тестах.
// TODO: Опиши Address и Parcel так, чтобы тесты прошли. Схема:
//     message Address { string city = 1; uint32 zip = 2; }
//     message Parcel { uint64 id = 1; string title = 2; repeated sint32 deltas = 4; bool fragile = 5; Address to = 6; }

#![allow(dead_code)]
use prost::Message;

#[derive(Clone, PartialEq, Default)]
struct Address {
    city: String,
    zip: u32,
}

#[derive(Clone, PartialEq, Default)]
struct Parcel {
    id: u64,
    title: String,
    deltas: Vec<i32>,
    fragile: bool,
    to: Option<Address>,
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Parcel {
        Parcel { id: 150, title: "box".into(), deltas: vec![1, -1, 300], fragile: true, to: Some(Address { city: "Oslo".into(), zip: 150 }) }
    }

    const SAMPLE_BYTES: [u8; 27] = [
        0x08, 0x96, 0x01, 0x12, 0x03, b'b', b'o', b'x', 0x22, 0x04, 0x02, 0x01, 0xd8, 0x04, 0x28, 0x01, 0x32, 0x09, 0x0a, 0x04, b'O', b's', b'l', b'o', 0x10, 0x96, 0x01,
    ];

    #[test]
    fn encodes_exactly_the_expected_bytes() {
        assert_eq!(sample().encode_to_vec(), SAMPLE_BYTES.to_vec(), "байты не совпали со схемой: проверь номера полей, типы (sint32 это не int32) и то, что repeated пишется упакованно");
        assert_eq!(sample().encoded_len(), SAMPLE_BYTES.len());
    }

    #[test]
    fn decodes_the_same_bytes_back() {
        assert_eq!(Parcel::decode(&SAMPLE_BYTES[..]).unwrap(), sample());
    }

    #[test]
    fn defaults_are_not_on_the_wire() {
        assert!(Parcel::default().encode_to_vec().is_empty(), "пустое сообщение proto3 это ноль байтов");
        let only_id = Parcel { id: 1, ..Parcel::default() };
        assert_eq!(only_id.encode_to_vec(), vec![0x08, 0x01]);
    }

    #[test]
    fn foreign_bytes_with_unknown_field_unpacked_list_and_partial_message() {
        let bytes = [0x32, 0x02, 0x10, 0x05, 0x08, 0x07, 0x20, 0x02, 0x20, 0x01, 0x78, 0x01];
        let parcel = Parcel::decode(&bytes[..]).unwrap();
        assert_eq!(parcel.id, 7);
        assert_eq!(parcel.deltas, vec![1, -1], "непакованный список тоже читается");
        assert_eq!(parcel.to, Some(Address { city: String::new(), zip: 5 }), "вложенное сообщение с одним полем, поле 15 пропущено");
        assert!(!parcel.fragile);
    }

    #[test]
    fn nested_message_presence_is_kept() {
        let empty_address = Parcel { to: Some(Address::default()), ..Parcel::default() };
        assert_eq!(empty_address.encode_to_vec(), vec![0x32, 0x00], "пустое, но заданное вложенное сообщение отличимо от отсутствующего");
        assert_eq!(Parcel::decode(&[0x32, 0x00][..]).unwrap().to, Some(Address::default()));
        assert_eq!(Parcel::decode(&[][..]).unwrap().to, None);
    }

    #[test]
    fn truncated_bytes_are_an_error() {
        for cut in [1, 4, 9, 15, 20, 26] {
            assert!(Parcel::decode(&SAMPLE_BYTES[..cut]).is_err(), "обрезка на {cut} байтах должна быть ошибкой");
        }
    }
}
