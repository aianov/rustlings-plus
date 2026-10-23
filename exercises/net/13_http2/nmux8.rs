// Настоящие HTTP/2-соединения не повторяют заголовки: и клиент, и сервер ведут динамическую таблицу, куда складывают встретившиеся
// пары, а потом ссылаются на них одним байтом. Индекс 62 это самая свежая запись, 63 предыдущая и так далее. У таблицы есть
// предельный размер в байтах (пара занимает длину имени, длину значения и ещё 32), и старые записи выбрасываются, когда не
// хватает места. Ошибиться в учёте размера значит потом ссылаться не на ту запись, и дамп превратится в мусор без единой ошибки.
// TODO: Допиши DynamicTable (insert с вытеснением, set_max_size, get) и Decoder::decode поверх таблицы. Хаффмана по-прежнему нет.

#![allow(dead_code)]
use std::collections::VecDeque;

#[derive(Debug, PartialEq)]
enum HpackError {
    Truncated,
    BadIndex(usize),
    BadInt,
    BadString,
    Huffman,
    SizeUpdate,
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

fn read_string(buf: &[u8]) -> Result<(String, usize), HpackError> {
    let first = *buf.first().ok_or(HpackError::Truncated)?;
    if first & 0x80 != 0 {
        return Err(HpackError::Huffman);
    }
    let (len, used) = decode_int(buf, 7)?;
    let end = used.checked_add(usize::try_from(len).map_err(|_| HpackError::Truncated)?).ok_or(HpackError::Truncated)?;
    let bytes = buf.get(used..end).ok_or(HpackError::Truncated)?;
    Ok((String::from_utf8(bytes.to_vec()).map_err(|_| HpackError::BadString)?, end))
}

struct DynamicTable {
    entries: VecDeque<Header>,
    size: usize,
    max_size: usize,
}

fn entry_size(name: &str, value: &str) -> usize {
    name.len() + value.len() + 32
}

impl DynamicTable {
    fn new(max_size: usize) -> DynamicTable {
        DynamicTable { entries: VecDeque::new(), size: 0, max_size }
    }

    fn size(&self) -> usize {
        self.size
    }

    fn len(&self) -> usize {
        self.entries.len()
    }

    fn insert(&mut self, name: String, value: String) {
        let _ = (name, value);
        todo!()
    }

    fn set_max_size(&mut self, new_max: usize) {
        let _ = new_max;
        todo!()
    }

    fn get(&self, newest_first: usize) -> Option<&Header> {
        let _ = newest_first;
        todo!()
    }
}

struct Decoder {
    table: DynamicTable,
    limit: usize,
}

impl Decoder {
    fn new(limit: usize) -> Decoder {
        Decoder { table: DynamicTable::new(limit), limit }
    }

    fn lookup(&self, index: u64) -> Result<Header, HpackError> {
        let bad = || HpackError::BadIndex(usize::try_from(index).unwrap_or(usize::MAX));
        match index {
            0 => Err(bad()),
            1..=61 => {
                let (name, value) = STATIC[index as usize - 1];
                Ok((name.to_string(), value.to_string()))
            }
            _ => self.table.get(index as usize - 62).cloned().ok_or_else(bad),
        }
    }

    fn decode(&mut self, block: &[u8]) -> Result<Vec<Header>, HpackError> {
        let _ = block;
        todo!()
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(name: &str, value: &str) -> Header {
        (name.to_string(), value.to_string())
    }

    fn string(s: &str) -> Vec<u8> {
        let mut out = Vec::new();
        encode_int(s.len() as u64, 7, 0, &mut out);
        out.extend_from_slice(s.as_bytes());
        out
    }

    fn indexed(index: u64) -> Vec<u8> {
        let mut out = Vec::new();
        encode_int(index, 7, 0x80, &mut out);
        out
    }

    fn add_new(name: &str, value: &str) -> Vec<u8> {
        let mut out = vec![0x40];
        out.extend(string(name));
        out.extend(string(value));
        out
    }

    fn add_named(index: u64, value: &str) -> Vec<u8> {
        let mut out = Vec::new();
        encode_int(index, 6, 0x40, &mut out);
        out.extend(string(value));
        out
    }

    fn size_update(size: u64) -> Vec<u8> {
        let mut out = Vec::new();
        encode_int(size, 5, 0x20, &mut out);
        out
    }

    #[test]
    fn rfc_request_sequence_without_huffman() {
        let mut decoder = Decoder::new(4096);
        let first = [0x82, 0x86, 0x84, 0x41, 0x0f, 0x77, 0x77, 0x77, 0x2e, 0x65, 0x78, 0x61, 0x6d, 0x70, 0x6c, 0x65, 0x2e, 0x63, 0x6f, 0x6d];
        assert_eq!(decoder.decode(&first), Ok(vec![h(":method", "GET"), h(":scheme", "http"), h(":path", "/"), h(":authority", "www.example.com")]));
        assert_eq!((decoder.table.len(), decoder.table.size()), (1, 57), "RFC 7541, C.3.1: в таблице одна запись размером 57");

        let second = [0x82, 0x86, 0x84, 0xbe, 0x58, 0x08, 0x6e, 0x6f, 0x2d, 0x63, 0x61, 0x63, 0x68, 0x65];
        assert_eq!(decoder.decode(&second), Ok(vec![h(":method", "GET"), h(":scheme", "http"), h(":path", "/"), h(":authority", "www.example.com"), h("cache-control", "no-cache")]), "0xbe это индекс 62: запись из прошлого запроса");
        assert_eq!((decoder.table.len(), decoder.table.size()), (2, 110));

        let third = [0x82, 0x87, 0x85, 0xbf, 0x40, 0x0a, 0x63, 0x75, 0x73, 0x74, 0x6f, 0x6d, 0x2d, 0x6b, 0x65, 0x79, 0x0c, 0x63, 0x75, 0x73, 0x74, 0x6f, 0x6d, 0x2d, 0x76, 0x61, 0x6c, 0x75, 0x65];
        assert_eq!(decoder.decode(&third), Ok(vec![h(":method", "GET"), h(":scheme", "https"), h(":path", "/index.html"), h(":authority", "www.example.com"), h("custom-key", "custom-value")]), "0xbf это индекс 63: старая запись сдвинулась, потому что 62 теперь cache-control");
        assert_eq!((decoder.table.len(), decoder.table.size()), (3, 164));
    }

    #[test]
    fn newest_entry_is_index_62_and_older_shift_up() {
        let mut decoder = Decoder::new(4096);
        let mut block = add_new("a", "1");
        block.extend(add_new("b", "2"));
        block.extend(add_new("c", "3"));
        decoder.decode(&block).unwrap();
        assert_eq!(decoder.decode(&[indexed(62), indexed(63), indexed(64)].concat()), Ok(vec![h("c", "3"), h("b", "2"), h("a", "1")]));
        decoder.decode(&add_new("d", "4")).unwrap();
        assert_eq!(decoder.decode(&indexed(62)), Ok(vec![h("d", "4")]));
        assert_eq!(decoder.decode(&indexed(65)), Ok(vec![h("a", "1")]));
        assert_eq!(decoder.decode(&indexed(66)), Err(HpackError::BadIndex(66)), "четыре записи: 66 уже нет");
    }

    #[test]
    fn name_can_be_taken_from_either_table() {
        let mut decoder = Decoder::new(4096);
        decoder.decode(&add_new("x-trace", "one")).unwrap();
        assert_eq!(decoder.decode(&add_named(62, "two")), Ok(vec![h("x-trace", "two")]), "имя из динамической таблицы");
        assert_eq!(decoder.decode(&add_named(31, "application/grpc")), Ok(vec![h("content-type", "application/grpc")]), "имя из статической");
        assert_eq!(decoder.table.len(), 3);
        assert_eq!(decoder.decode(&indexed(62)), Ok(vec![h("content-type", "application/grpc")]));
    }

    #[test]
    fn eviction_drops_the_oldest_entries_first() {
        let mut decoder = Decoder::new(256);
        for n in 0..5 {
            decoder.decode(&add_new("name-aaaa", &format!("value-{n:03}"))).unwrap();
        }
        assert_eq!(entry_size("name-aaaa", "value-000"), 50);
        assert_eq!((decoder.table.len(), decoder.table.size()), (5, 250), "пять записей по 50 байтов помещаются в 256");
        decoder.decode(&add_new("name-aaaa", "value-005")).unwrap();
        assert_eq!((decoder.table.len(), decoder.table.size()), (5, 250), "шестая вытеснила самую старую");
        assert_eq!(decoder.decode(&indexed(66)), Ok(vec![h("name-aaaa", "value-001")]), "самая старая из оставшихся это вторая вставка");
        assert_eq!(decoder.decode(&indexed(67)), Err(HpackError::BadIndex(67)));
    }

    #[test]
    fn eviction_makes_room_for_the_new_entry_even_if_it_takes_several() {
        let mut decoder = Decoder::new(200);
        for n in 0..3 {
            decoder.decode(&add_new("k", &format!("{n}"))).unwrap();
        }
        assert_eq!(decoder.table.size(), 3 * 34);
        let big = "z".repeat(120);
        decoder.decode(&add_new("big", &big)).unwrap();
        assert_eq!(decoder.table.len(), 2, "для записи на 155 байтов пришлось выбросить две старые");
        assert_eq!(decoder.table.size(), 34 + 155);
    }

    #[test]
    fn entry_bigger_than_the_whole_table_empties_it() {
        let mut decoder = Decoder::new(100);
        decoder.decode(&add_new("a", "1")).unwrap();
        assert_eq!(decoder.table.len(), 1);
        let block = add_new("huge", &"v".repeat(100));
        assert_eq!(decoder.decode(&block), Ok(vec![h("huge", &"v".repeat(100))]), "заголовок при этом всё равно возвращается");
        assert_eq!((decoder.table.len(), decoder.table.size()), (0, 0), "запись больше таблицы не добавляется, а таблица очищается");
    }

    #[test]
    fn unindexed_literals_do_not_touch_the_table() {
        let mut decoder = Decoder::new(4096);
        let mut block = vec![0x00];
        block.extend(string("plain"));
        block.extend(string("1"));
        block.push(0x10);
        block.extend(string("secret"));
        block.extend(string("2"));
        assert_eq!(decoder.decode(&block), Ok(vec![h("plain", "1"), h("secret", "2")]));
        assert_eq!(decoder.table.len(), 0);
    }

    #[test]
    fn table_size_updates() {
        let mut decoder = Decoder::new(4096);
        decoder.decode(&add_new("a", "1")).unwrap();
        decoder.decode(&add_new("b", "2")).unwrap();
        let mut block = size_update(34);
        block.extend(indexed(2));
        assert_eq!(decoder.decode(&block), Ok(vec![h(":method", "GET")]));
        assert_eq!((decoder.table.len(), decoder.table.size()), (1, 34), "уменьшение предела вытесняет лишнее, оставляя самое свежее");
        assert_eq!(decoder.decode(&indexed(62)), Ok(vec![h("b", "2")]));
        let mut block = size_update(0);
        block.extend(size_update(100));
        assert!(decoder.decode(&block).is_ok(), "в начале блока допустимо несколько обновлений");
        assert_eq!(decoder.table.len(), 0);
    }

    #[test]
    fn table_size_update_errors() {
        let mut decoder = Decoder::new(1000);
        assert_eq!(decoder.decode(&size_update(1001)), Err(HpackError::SizeUpdate), "выше согласованного предела (SETTINGS_HEADER_TABLE_SIZE)");
        let mut late = indexed(2);
        late.extend(size_update(10));
        assert_eq!(decoder.decode(&late), Err(HpackError::SizeUpdate), "обновление размера допустимо только в начале блока");
    }

    #[test]
    fn dynamic_index_out_of_range_and_huffman() {
        let mut decoder = Decoder::new(4096);
        assert_eq!(decoder.decode(&indexed(62)), Err(HpackError::BadIndex(62)));
        assert_eq!(decoder.decode(&indexed(0)), Err(HpackError::BadIndex(0)));
        assert_eq!(decoder.decode(&[0x41, 0x8c, 0xf1, 0xe3, 0xc2, 0xe5, 0xf2, 0x3a, 0x6b, 0xa0, 0xab, 0x90, 0xf4, 0xff]), Err(HpackError::Huffman));
        assert_eq!(decoder.decode(&[0x40, 0x01]), Err(HpackError::Truncated));
    }

    #[test]
    fn two_connections_have_independent_tables() {
        let mut a = Decoder::new(4096);
        let mut b = Decoder::new(4096);
        a.decode(&add_new("x", "1")).unwrap();
        assert_eq!(b.decode(&indexed(62)), Err(HpackError::BadIndex(62)), "таблица принадлежит соединению и направлению, а не процессу");
    }
}
