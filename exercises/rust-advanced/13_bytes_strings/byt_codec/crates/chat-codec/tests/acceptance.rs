use chat_codec::*;

fn msg(flags: u8, reply_to: Option<u32>, text: &str) -> Message {
    Message { flags, reply_to, text: text.to_string() }
}

#[test]
fn a_plain_message_has_the_documented_layout() {
    assert_eq!(encode(&msg(0, None, "hi")), Ok(vec![0, 0, 2, b'h', b'i']));
    assert_eq!(encode(&msg(EDITED | SILENT, None, "")), Ok(vec![5, 0, 0]));
}

#[test]
fn a_reply_carries_the_original_id() {
    let bytes = encode(&msg(REPLY, Some(0x0102_0304), "ok")).unwrap();
    assert_eq!(bytes, vec![2, 0, 2, 1, 2, 3, 4, b'o', b'k']);
}

#[test]
fn encoding_refuses_inconsistent_messages() {
    assert_eq!(encode(&msg(REPLY, None, "x")), Err(EncodeError::ReplyMismatch));
    assert_eq!(encode(&msg(0, Some(1), "x")), Err(EncodeError::ReplyMismatch));
    assert_eq!(encode(&msg(0b1000_0000, None, "x")), Err(EncodeError::UnknownFlags(0b1000_0000)));
}

#[test]
fn length_is_counted_in_bytes() {
    assert!(encode(&msg(0, None, &"a".repeat(MAX_TEXT))).is_ok());
    assert_eq!(encode(&msg(0, None, &"a".repeat(MAX_TEXT + 1))), Err(EncodeError::TooLong(MAX_TEXT + 1)));
    let cyrillic = "я".repeat(MAX_TEXT / 2 + 1);
    assert_eq!(cyrillic.chars().count(), 2049);
    assert_eq!(encode(&msg(0, None, &cyrillic)), Err(EncodeError::TooLong(4098)), "2049 буквы это 4098 байт");
    assert!(encode(&msg(0, None, &"я".repeat(MAX_TEXT / 2))).is_ok());
}

#[test]
fn messages_round_trip() {
    for m in [
        msg(0, None, ""),
        msg(0, None, "hello"),
        msg(EDITED, None, "Привет, мир 👋"),
        msg(REPLY | SILENT, Some(u32::MAX), "ответ 🌍"),
        msg(REPLY, Some(0), &"x".repeat(MAX_TEXT)),
    ] {
        let bytes = encode(&m).unwrap();
        assert_eq!(decode(&bytes), Ok((m, bytes.len())));
    }
}

#[test]
fn decoding_ignores_the_tail_and_reports_the_size() {
    let mut bytes = encode(&msg(0, None, "one")).unwrap();
    let size = bytes.len();
    bytes.extend_from_slice(&encode(&msg(0, None, "two")).unwrap());
    assert_eq!(decode(&bytes), Ok((msg(0, None, "one"), size)));
}

#[test]
fn every_prefix_asks_for_the_right_number_of_bytes() {
    let bytes = encode(&msg(REPLY, Some(77), "привет")).unwrap();
    for cut in 0..bytes.len() {
        let needed = if cut < 3 { 3 - cut } else { bytes.len() - cut };
        assert_eq!(decode(&bytes[..cut]), Err(DecodeError::Incomplete { needed }), "обрезано после {cut} байт");
    }
}

#[test]
fn garbage_is_rejected_early() {
    assert_eq!(decode(&[0x80]), Err(DecodeError::UnknownFlags(0x80)), "лишние флаги видны по одному байту");
    assert_eq!(decode(&[0, 0x13, 0x88]), Err(DecodeError::TooLong(5000)), "5000 байт ждать не нужно");
    assert_eq!(decode(&[0, 0x10, 0x01]), Err(DecodeError::TooLong(4097)));
    assert_eq!(decode(&[0, 0, 2, 0xff, 0xfe]), Err(DecodeError::BadUtf8));
    assert_eq!(decode(&[0, 0, 2, 0xd0, 0xd0]), Err(DecodeError::BadUtf8), "две «половинки» букв: все байты на месте, значит это мусор");
}

#[test]
fn the_decoder_never_panics() {
    let mut x = 12345_u32;
    for _ in 0..3000 {
        let len = (x % 12) as usize;
        let mut buf = Vec::new();
        for _ in 0..len {
            x = x.wrapping_mul(1_103_515_245).wrapping_add(12345);
            buf.push((x >> 16) as u8);
        }
        let _ = decode(&buf);
    }
}

#[test]
fn a_stream_assembles_messages_from_single_bytes() {
    let a = msg(0, None, "Привет 👋");
    let b = msg(REPLY, Some(9), "и тебе 🌍");
    let mut wire = encode(&a).unwrap();
    wire.extend_from_slice(&encode(&b).unwrap());
    let mut stream = Stream::new();
    let mut got = Vec::new();
    for byte in &wire {
        stream.feed(std::slice::from_ref(byte));
        while let Some(m) = stream.next_message().expect("данные корректны") {
            got.push(m);
        }
    }
    assert_eq!(got, vec![a, b]);
    assert_eq!(stream.buffered(), 0, "прочитанные байты должны убираться из буфера");
}

#[test]
fn a_stream_returns_several_messages_from_one_chunk() {
    let mut wire = Vec::new();
    for text in ["a", "bb", "ccc"] {
        wire.extend_from_slice(&encode(&msg(0, None, text)).unwrap());
    }
    wire.extend_from_slice(&[0, 0]);
    let mut stream = Stream::new();
    stream.feed(&wire);
    assert_eq!(stream.next_message().unwrap().map(|m| m.text), Some("a".to_string()));
    assert_eq!(stream.next_message().unwrap().map(|m| m.text), Some("bb".to_string()));
    assert_eq!(stream.next_message().unwrap().map(|m| m.text), Some("ccc".to_string()));
    assert_eq!(stream.next_message(), Ok(None), "остались два байта чужого заголовка");
    assert_eq!(stream.buffered(), 2);
}

#[test]
fn a_stream_reports_garbage_and_keeps_reporting() {
    let mut stream = Stream::new();
    stream.feed(&[0xf0]);
    assert_eq!(stream.next_message(), Err(DecodeError::UnknownFlags(0xf0)));
    assert_eq!(stream.next_message(), Err(DecodeError::UnknownFlags(0xf0)), "данные не должны молча пропасть");
}

#[test]
fn hexdump_shows_offsets_hex_and_ascii() {
    assert_eq!(hexdump(b""), "");
    let expected = format!("00000000  41 42 {}|AB|\n", "   ".repeat(14));
    assert_eq!(hexdump(b"AB"), expected);
    let mut data: Vec<u8> = (0x41..0x51).collect();
    data.extend_from_slice(&[0x00, 0xff, b' ', b'z']);
    let dump = hexdump(&data);
    let lines: Vec<&str> = dump.lines().collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0], "00000000  41 42 43 44 45 46 47 48 49 4a 4b 4c 4d 4e 4f 50 |ABCDEFGHIJKLMNOP|");
    assert_eq!(lines[1], format!("00000010  00 ff 20 7a {}|.. z|", "   ".repeat(12)));
    assert!(dump.ends_with("|\n"));
}

#[test]
fn preview_fits_the_limit_on_a_character_boundary() {
    assert_eq!(preview("short", 10), "short");
    assert_eq!(preview("Привет, мир", 12), "Прив…");
    assert_eq!(preview("👍👍👍👍👍", 10), "👍…");
    assert_eq!(preview("long text", 2), "");
    for limit in 0..40 {
        assert!(preview("Привет 👋 это длинное сообщение", limit).len() <= limit);
    }
}
