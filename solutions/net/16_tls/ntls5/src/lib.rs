// Слушатель между клиентом и сервером видит байты. Научись читать то, что он увидит:
// есть ли в записи пароль, похожа ли она на TLS и к какому имени идёт клиент (SNI).
pub mod tap;

pub fn contains(hay: &[u8], needle: &[u8]) -> bool {
    needle.is_empty() || hay.windows(needle.len()).any(|w| w == needle)
}

pub fn looks_like_tls(bytes: &[u8]) -> bool {
    bytes.len() >= 3 && bytes[0] == 22 && bytes[1] == 3
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.0.len() < n {
            return None;
        }
        let (head, tail) = self.0.split_at(n);
        self.0 = tail;
        Some(head)
    }

    fn u8(&mut self) -> Option<usize> {
        self.take(1).map(|b| b[0] as usize)
    }

    fn u16(&mut self) -> Option<usize> {
        self.take(2).map(|b| u16::from_be_bytes([b[0], b[1]]) as usize)
    }

    fn u24(&mut self) -> Option<usize> {
        self.take(3).map(|b| ((b[0] as usize) << 16) | ((b[1] as usize) << 8) | b[2] as usize)
    }
}

pub fn sni(bytes: &[u8]) -> Option<String> {
    let mut r = Reader(bytes);
    if r.u8()? != 22 {
        return None;
    }
    r.take(2)?;
    let record_len = r.u16()?;
    let mut r = Reader(&r.0[..record_len.min(r.0.len())]);
    if r.u8()? != 1 {
        return None;
    }
    r.u24()?;
    r.take(2 + 32)?;
    let session = r.u8()?;
    r.take(session)?;
    let suites = r.u16()?;
    r.take(suites)?;
    let compression = r.u8()?;
    r.take(compression)?;
    let extensions_len = r.u16()?;
    let mut ext = Reader(r.take(extensions_len)?);
    while let (Some(kind), Some(len)) = (ext.u16(), ext.u16()) {
        let data = ext.take(len)?;
        if kind == 0 {
            let mut d = Reader(data);
            d.u16()?;
            if d.u8()? != 0 {
                return None;
            }
            let name_len = d.u16()?;
            return String::from_utf8(d.take(name_len)?.to_vec()).ok();
        }
    }
    None
}
