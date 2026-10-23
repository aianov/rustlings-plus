pub const HEADER_LEN: usize = 7;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Header {
    pub id: u32,
    pub len: u16,
    pub flags: u8,
}

pub fn encode(header: &Header) -> [u8; HEADER_LEN] {
    let mut out = [0; HEADER_LEN];
    out[..4].copy_from_slice(&header.id.to_be_bytes());
    out[4..6].copy_from_slice(&header.len.to_be_bytes());
    out[6] = header.flags;
    out
}

pub fn decode(buf: &[u8]) -> Option<Header> {
    let head = buf.get(..HEADER_LEN)?;
    Some(Header {
        id: u32::from_be_bytes(head[..4].try_into().ok()?),
        len: u16::from_be_bytes(head[4..6].try_into().ok()?),
        flags: head[6],
    })
}

pub fn read_u32_le(buf: &[u8], at: usize) -> Option<u32> {
    let end = at.checked_add(4)?;
    Some(u32::from_le_bytes(buf.get(at..end)?.try_into().ok()?))
}

fn main() {
    println!("{:?}", encode(&Header { id: 1, len: 2, flags: 3 }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wire_layout_is_big_endian() {
        let h = Header { id: 0x0102_0304, len: 0x0a0b, flags: 0xff };
        assert_eq!(encode(&h), [1, 2, 3, 4, 0x0a, 0x0b, 0xff], "старший байт идёт первым");
    }

    #[test]
    fn decode_is_the_inverse_of_encode() {
        for h in [
            Header { id: 0, len: 0, flags: 0 },
            Header { id: u32::MAX, len: u16::MAX, flags: u8::MAX },
            Header { id: 258, len: 1, flags: 7 },
        ] {
            assert_eq!(decode(&encode(&h)), Some(h));
        }
    }

    #[test]
    fn extra_bytes_are_ignored_and_short_buffers_refused() {
        let mut buf = encode(&Header { id: 5, len: 6, flags: 1 }).to_vec();
        buf.extend_from_slice(b"text follows");
        assert_eq!(decode(&buf), Some(Header { id: 5, len: 6, flags: 1 }));
        assert_eq!(decode(&buf[..6]), None, "шесть байт из семи: заголовка ещё нет");
        assert_eq!(decode(&[]), None);
    }

    #[test]
    fn little_endian_reads_from_any_offset() {
        let buf = [9, 0x01, 0x02, 0x03, 0x04, 9];
        assert_eq!(read_u32_le(&buf, 1), Some(0x0403_0201), "младший байт первым");
        assert_eq!(read_u32_le(&buf, 2), Some(0x0904_0302), "ровно до конца буфера: последние четыре байта");
        assert_eq!(read_u32_le(&buf, 3), None);
        assert_eq!(read_u32_le(&buf, usize::MAX), None, "at + 4 не должно переполняться");
    }
}
