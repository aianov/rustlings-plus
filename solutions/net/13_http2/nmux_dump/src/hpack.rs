#![allow(dead_code)]

use std::collections::VecDeque;

#[derive(Debug, PartialEq)]
pub enum HpackError {
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

pub type Header = (String, String);

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

pub struct DynamicTable {
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
        let new_size = entry_size(&name, &value);
        if new_size > self.max_size {
            self.entries.clear();
            self.size = 0;
            return;
        }
        while self.size + new_size > self.max_size {
            self.evict_oldest();
        }
        self.size += new_size;
        self.entries.push_front((name, value));
    }

    fn evict_oldest(&mut self) {
        if let Some((name, value)) = self.entries.pop_back() {
            self.size -= entry_size(&name, &value);
        }
    }

    fn set_max_size(&mut self, new_max: usize) {
        self.max_size = new_max;
        while self.size > self.max_size {
            self.evict_oldest();
        }
    }

    fn get(&self, newest_first: usize) -> Option<&Header> {
        self.entries.get(newest_first)
    }
}

pub struct Decoder {
    table: DynamicTable,
    limit: usize,
}

impl Decoder {
    pub fn new(limit: usize) -> Decoder {
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

    pub fn decode(&mut self, block: &[u8]) -> Result<Vec<Header>, HpackError> {
        let mut headers = Vec::new();
        let mut pos = 0;
        let mut at_start = true;
        while pos < block.len() {
            let first = block[pos];
            if first & 0xe0 == 0x20 {
                let (size, used) = decode_int(&block[pos..], 5)?;
                if !at_start || size > self.limit as u64 {
                    return Err(HpackError::SizeUpdate);
                }
                self.table.set_max_size(size as usize);
                pos += used;
                continue;
            }
            at_start = false;
            if first & 0x80 != 0 {
                let (index, used) = decode_int(&block[pos..], 7)?;
                headers.push(self.lookup(index)?);
                pos += used;
                continue;
            }
            let (prefix, indexing) = if first & 0x40 != 0 { (6, true) } else { (4, false) };
            let (index, used) = decode_int(&block[pos..], prefix)?;
            pos += used;
            let name = if index == 0 {
                let (name, used) = read_string(&block[pos..])?;
                pos += used;
                name
            } else {
                self.lookup(index)?.0
            };
            let (value, used) = read_string(&block[pos..])?;
            pos += used;
            if indexing {
                self.table.insert(name.clone(), value.clone());
            }
            headers.push((name, value));
        }
        Ok(headers)
    }
}
