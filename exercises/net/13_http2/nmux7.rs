// Заголовки gRPC-вызова (:path, content-type, grpc-timeout) едут в сжатом виде HPACK. Чтобы читать дамп разговора, нужно уметь
// три вещи: ссылку на строку статической таблицы из 61 записи (одним байтом), литерал, у которого имя взято из таблицы, и
// литерал с новым именем. Сжатие по Хаффману и динамическую таблицу оставим на потом: встретив их, надо честно сказать «не умею».
// TODO: Допиши decode_block (разбор блока в список заголовков) и encode_headers (запись списка с максимальным использованием
// статической таблицы, но без индексации и без Хаффмана). Таблица и запись целых чисел уже есть.

#![allow(dead_code)]

#[derive(Debug, PartialEq)]
enum HpackError {
    Truncated,
    BadIndex(usize),
    BadInt,
    BadString,
    Huffman,
    Unsupported,
}

const STATIC: [(&str, &str); 61] = [
    (":authority", ""),
    (":method", "GET"),
    (":method", "POST"),
    (":path", "/"),
    (":path", "/index.html"),
    (":scheme", "http"),
    (":scheme", "https"),
    (":status", "200"),
    (":status", "204"),
    (":status", "206"),
    (":status", "304"),
    (":status", "400"),
    (":status", "404"),
    (":status", "500"),
    ("accept-charset", ""),
    ("accept-encoding", "gzip, deflate"),
    ("accept-language", ""),
    ("accept-ranges", ""),
    ("accept", ""),
    ("access-control-allow-origin", ""),
    ("age", ""),
    ("allow", ""),
    ("authorization", ""),
    ("cache-control", ""),
    ("content-disposition", ""),
    ("content-encoding", ""),
    ("content-language", ""),
    ("content-length", ""),
    ("content-location", ""),
    ("content-range", ""),
    ("content-type", ""),
    ("cookie", ""),
    ("date", ""),
    ("etag", ""),
    ("expect", ""),
    ("expires", ""),
    ("from", ""),
    ("host", ""),
    ("if-match", ""),
    ("if-modified-since", ""),
    ("if-none-match", ""),
    ("if-range", ""),
    ("if-unmodified-since", ""),
    ("last-modified", ""),
    ("link", ""),
    ("location", ""),
    ("max-forwards", ""),
    ("proxy-authenticate", ""),
    ("proxy-authorization", ""),
    ("range", ""),
    ("referer", ""),
    ("refresh", ""),
    ("retry-after", ""),
    ("server", ""),
    ("set-cookie", ""),
    ("strict-transport-security", ""),
    ("transfer-encoding", ""),
    ("user-agent", ""),
    ("vary", ""),
    ("via", ""),
    ("www-authenticate", ""),
];

fn encode_int(value: u64, prefix_bits: u8, flags: u8, out: &mut Vec<u8>) {
    let max = (1u64 << prefix_bits) - 1;
    if value < max {
        out.push(flags | value as u8);
        return;
    }
    out.push(flags | max as u8);
    let mut rest = value - max;
    while rest >= 128 {
        out.push((rest % 128) as u8 | 0x80);
        rest /= 128;
    }
    out.push(rest as u8);
}

fn decode_int(buf: &[u8], prefix_bits: u8) -> Result<(u64, usize), HpackError> {
    let first = *buf.first().ok_or(HpackError::Truncated)?;
    let max = (1u64 << prefix_bits) - 1;
    let mut value = u64::from(first) & max;
    if value < max {
        return Ok((value, 1));
    }
    let mut shift = 0u32;
    for (i, &byte) in buf[1..].iter().enumerate() {
        if shift > 56 {
            return Err(HpackError::BadInt);
        }
        value += u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Ok((value, i + 2));
        }
        shift += 7;
    }
    Err(HpackError::Truncated)
}

type Header = (String, String);

fn decode_block(block: &[u8]) -> Result<Vec<Header>, HpackError> {
    let _ = block;
    todo!()
}

fn encode_headers(headers: &[(&str, &str)]) -> Vec<u8> {
    let _ = headers;
    todo!()
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(name: &str, value: &str) -> Header {
        (name.to_string(), value.to_string())
    }

    #[test]
    fn indexed_fields_from_the_static_table() {
        assert_eq!(decode_block(&[0x82]), Ok(vec![h(":method", "GET")]));
        assert_eq!(decode_block(&[0x83, 0x86, 0x84]), Ok(vec![h(":method", "POST"), h(":scheme", "http"), h(":path", "/")]));
        assert_eq!(decode_block(&[0x80 | 61]), Ok(vec![h("www-authenticate", "")]));
        assert_eq!(decode_block(&[]), Ok(vec![]));
    }

    #[test]
    fn rfc_literal_without_indexing() {
        let block = [0x04, 0x0c, 0x2f, 0x73, 0x61, 0x6d, 0x70, 0x6c, 0x65, 0x2f, 0x70, 0x61, 0x74, 0x68];
        assert_eq!(decode_block(&block), Ok(vec![h(":path", "/sample/path")]), "RFC 7541, C.2.2: имя по индексу 4, значение литералом");
    }

    #[test]
    fn rfc_literal_never_indexed() {
        let block = [0x10, 0x08, 0x70, 0x61, 0x73, 0x73, 0x77, 0x6f, 0x72, 0x64, 0x06, 0x73, 0x65, 0x63, 0x72, 0x65, 0x74];
        assert_eq!(decode_block(&block), Ok(vec![h("password", "secret")]), "RFC 7541, C.2.3: имя и значение литералами, помечено как «никогда не индексировать»");
    }

    #[test]
    fn grpc_style_block() {
        let mut block = vec![0x83, 0x86];
        block.extend(encode_headers(&[(":path", "/echo.Echo/Say"), ("content-type", "application/grpc"), ("te", "trailers"), ("grpc-timeout", "100m")]));
        let headers = decode_block(&block).unwrap();
        assert_eq!(
            headers,
            vec![h(":method", "POST"), h(":scheme", "http"), h(":path", "/echo.Echo/Say"), h("content-type", "application/grpc"), h("te", "trailers"), h("grpc-timeout", "100m")]
        );
    }

    #[test]
    fn encoder_uses_the_table_as_much_as_possible() {
        assert_eq!(encode_headers(&[(":method", "GET")]), vec![0x82], "полное совпадение даёт один байт");
        assert_eq!(encode_headers(&[(":method", "POST"), (":scheme", "https"), (":status", "404")]), vec![0x83, 0x87, 0x8d]);
        assert_eq!(encode_headers(&[(":path", "/sample/path")]), vec![0x04, 0x0c, 0x2f, 0x73, 0x61, 0x6d, 0x70, 0x6c, 0x65, 0x2f, 0x70, 0x61, 0x74, 0x68], "совпало только имя: берём наименьший индекс (4)");
        assert_eq!(encode_headers(&[("te", "trailers")]), vec![0x00, 0x02, b't', b'e', 0x08, b't', b'r', b'a', b'i', b'l', b'e', b'r', b's'], "имени нет в таблице: оба строки литералом");
        assert_eq!(encode_headers(&[("content-length", "0")]), vec![0x0f, 0x0d, 0x01, b'0'], "индекс 28 не влезает в 4 бита префикса и записывается продолжением");
    }

    #[test]
    fn long_values_use_multi_byte_lengths() {
        let value = "x".repeat(300);
        let block = encode_headers(&[("x-big", &value)]);
        assert_eq!(decode_block(&block), Ok(vec![h("x-big", &value)]));
        let name_indexed = encode_headers(&[("cookie", &value)]);
        assert_eq!(decode_block(&name_indexed), Ok(vec![h("cookie", &value)]));
    }

    #[test]
    fn what_we_do_not_support_is_reported_honestly() {
        assert_eq!(decode_block(&[0x40, 0x01, b'a', 0x01, b'b']), Err(HpackError::Unsupported), "литерал с индексацией требует динамической таблицы");
        assert_eq!(decode_block(&[0x20]), Err(HpackError::Unsupported), "изменение размера таблицы");
        assert_eq!(decode_block(&[0x80 | 62]), Err(HpackError::Unsupported), "индекс 62 и выше это динамическая таблица");
        assert_eq!(decode_block(&[0x04, 0x8c, 0xf1, 0xe3, 0xc2, 0xe5, 0xf2, 0x3a, 0x6b, 0xa0, 0xab, 0x90, 0xf4, 0xff]), Err(HpackError::Huffman), "значение сжато по Хаффману");
    }

    #[test]
    fn malformed_blocks() {
        assert_eq!(decode_block(&[0x80]), Err(HpackError::BadIndex(0)), "индекс 0 не бывает");
        assert_eq!(decode_block(&[0x04]), Err(HpackError::Truncated));
        assert_eq!(decode_block(&[0x04, 0x05, b'a', b'b']), Err(HpackError::Truncated), "строка короче заявленной");
        assert_eq!(decode_block(&[0x00, 0x01, b'a']), Err(HpackError::Truncated), "после имени нет значения");
        assert_eq!(decode_block(&[0x04, 0x02, 0xff, 0xfe]), Err(HpackError::BadString), "значение не UTF-8");
        assert_eq!(decode_block(&[0x04, 0x7f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x7f]), Err(HpackError::Truncated), "огромная длина строки не должна вызывать ни панику, ни выделение памяти");
    }

    #[test]
    fn encode_then_decode_roundtrip() {
        let headers = [(":status", "200"), ("content-type", "application/grpc"), ("grpc-status", "0"), ("grpc-message", ""), (":authority", "example.test:50051"), ("x-token", "a b c")];
        let decoded = decode_block(&encode_headers(&headers)).unwrap();
        let expected: Vec<Header> = headers.iter().map(|(n, v)| h(n, v)).collect();
        assert_eq!(decoded, expected);
    }
}
