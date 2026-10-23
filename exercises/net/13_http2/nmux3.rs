// Соединение HTTP/2 открывается рукопожатием. Клиент первым делом шлёт фиксированную строку-прелюдию (по ней сразу видно, что с
// другой стороны не HTTP/1.1), а следом кадр SETTINGS с параметрами. Принимающий обязан применить параметры и подтвердить их
// пустым SETTINGS с флагом ACK. Прелюдия приходит из сети порциями, и параметры пишет чужая сторона, поэтому всё надо проверять.
// TODO: Допиши check_preface (хватает ли байтов, совпадает ли начало) и apply_settings (применить пары по порядку, отвергнуть
// недопустимые значения, неизвестные параметры игнорировать).

#![allow(dead_code)]

const PREFACE: &[u8] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";

#[derive(Debug, PartialEq)]
enum Preface {
    Complete,
    NeedMore,
    Invalid,
}

#[derive(Debug, PartialEq, Clone)]
struct Settings {
    header_table_size: u32,
    enable_push: bool,
    max_concurrent_streams: Option<u32>,
    initial_window_size: u32,
    max_frame_size: u32,
    max_header_list_size: Option<u32>,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings { header_table_size: 4096, enable_push: true, max_concurrent_streams: None, initial_window_size: 65_535, max_frame_size: 16_384, max_header_list_size: None }
    }
}

const PROTOCOL_ERROR: u32 = 1;
const FLOW_CONTROL_ERROR: u32 = 3;

#[derive(Debug, PartialEq)]
struct ConnError(u32);

fn check_preface(buf: &[u8]) -> Preface {
    let _ = buf;
    todo!()
}

fn apply_settings(current: &mut Settings, entries: &[(u16, u32)]) -> Result<(), ConnError> {
    let _ = (current, entries);
    todo!()
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preface_arrives_in_pieces() {
        assert_eq!(check_preface(PREFACE), Preface::Complete);
        for cut in 0..PREFACE.len() {
            assert_eq!(check_preface(&PREFACE[..cut]), Preface::NeedMore, "на {cut} байтах прелюдия ещё не закончилась, но пока не нарушена");
        }
    }

    #[test]
    fn extra_bytes_after_the_preface_are_fine() {
        let mut buf = PREFACE.to_vec();
        buf.extend_from_slice(&[0, 0, 0, 4, 0, 0, 0, 0, 0]);
        assert_eq!(check_preface(&buf), Preface::Complete);
    }

    #[test]
    fn wrong_bytes_are_invalid_as_early_as_possible() {
        assert_eq!(check_preface(b"GET / HTTP/1.1\r\n\r\n"), Preface::Invalid, "это HTTP/1.1: клиент не говорит на HTTP/2");
        assert_eq!(check_preface(b"G"), Preface::Invalid, "достаточно первого неверного байта, ждать остальных не нужно");
        let mut almost = PREFACE.to_vec();
        almost[20] = b'X';
        assert_eq!(check_preface(&almost), Preface::Invalid);
        assert_eq!(check_preface(&almost[..21]), Preface::Invalid, "неверный байт уже виден, хоть прелюдия и не дочитана");
    }

    #[test]
    fn defaults_come_from_the_rfc() {
        let s = Settings::default();
        assert_eq!((s.header_table_size, s.enable_push, s.max_concurrent_streams, s.initial_window_size, s.max_frame_size), (4096, true, None, 65_535, 16_384));
    }

    #[test]
    fn entries_apply_in_order_and_the_last_wins() {
        let mut s = Settings::default();
        apply_settings(&mut s, &[(3, 100), (4, 1_000_000), (3, 250), (5, 32_768), (1, 0), (2, 0), (6, 8192)]).unwrap();
        assert_eq!(s, Settings { header_table_size: 0, enable_push: false, max_concurrent_streams: Some(250), initial_window_size: 1_000_000, max_frame_size: 32_768, max_header_list_size: Some(8192) });
    }

    #[test]
    fn unknown_ids_are_ignored() {
        let mut s = Settings::default();
        apply_settings(&mut s, &[(0, 5), (7, 1), (0xffff, 9), (3, 10)]).unwrap();
        assert_eq!(s.max_concurrent_streams, Some(10));
        assert_eq!(Settings { max_concurrent_streams: Some(10), ..Settings::default() }, s, "остальное не меняется");
    }

    #[test]
    fn invalid_values_are_connection_errors() {
        for (entries, code) in [
            (vec![(2u16, 2u32)], PROTOCOL_ERROR),
            (vec![(4, 0x8000_0000)], FLOW_CONTROL_ERROR),
            (vec![(5, 16_383)], PROTOCOL_ERROR),
            (vec![(5, 16_777_216)], PROTOCOL_ERROR),
        ] {
            let mut s = Settings::default();
            assert_eq!(apply_settings(&mut s, &entries), Err(ConnError(code)), "{entries:?}");
        }
        let mut s = Settings::default();
        assert!(apply_settings(&mut s, &[(5, 16_777_215), (4, 0x7fff_ffff), (2, 1)]).is_ok(), "границы допустимого включены");
        assert!(apply_settings(&mut s, &[(5, 16_384), (4, 0)]).is_ok());
    }

    #[test]
    fn a_bad_entry_leaves_the_settings_untouched() {
        let mut s = Settings::default();
        let before = s.clone();
        let result = apply_settings(&mut s, &[(3, 7), (4, 5000), (5, 1)]);
        assert_eq!(result, Err(ConnError(PROTOCOL_ERROR)));
        assert_eq!(s, before, "кадр с плохим параметром отвергается целиком: половина применённых настроек хуже, чем ни одной");
    }
}
