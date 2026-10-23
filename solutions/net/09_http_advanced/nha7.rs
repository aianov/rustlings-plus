#![allow(dead_code)]

#[derive(Debug, Clone, Default)]
struct Req {
    method: String,
    headers: Vec<(String, String)>,
}

#[allow(dead_code)]
impl Req {
    fn new(method: &str) -> Req {
        Req { method: method.to_string(), headers: Vec::new() }
    }

    fn with(mut self, name: &str, value: &str) -> Req {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }

    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Resp {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

#[allow(dead_code)]
impl Resp {
    fn new(status: u16) -> Resp {
        Resp { status, headers: Vec::new(), body: Vec::new() }
    }

    fn with(mut self, name: &str, value: &str) -> Resp {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }

    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    fn all(&self, name: &str) -> Vec<&str> {
        self.headers.iter().filter(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str()).collect()
    }

    fn tokens(&self, name: &str) -> Vec<String> {
        self.all(name).iter().flat_map(|v| v.split(',')).map(|t| t.trim().to_ascii_lowercase()).filter(|t| !t.is_empty()).collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RangeSpec {
    Full,
    Partial { start: u64, end: u64 },
    Unsatisfiable,
}

fn parse_range(header: Option<&str>, len: u64) -> RangeSpec {
    let Some(header) = header else { return RangeSpec::Full };
    let Some(spec) = header.trim().strip_prefix("bytes=") else { return RangeSpec::Full };
    if spec.contains(',') {
        return RangeSpec::Full;
    }
    let Some((first, last)) = spec.trim().split_once('-') else { return RangeSpec::Full };
    let number = |s: &str| -> Option<u64> { if !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) { s.parse().ok() } else { None } };
    match (first.is_empty(), last.is_empty()) {
        (true, true) => RangeSpec::Full,
        (true, false) => match number(last) {
            None => RangeSpec::Full,
            Some(0) => RangeSpec::Unsatisfiable,
            Some(_) if len == 0 => RangeSpec::Unsatisfiable,
            Some(n) => RangeSpec::Partial { start: len.saturating_sub(n), end: len - 1 },
        },
        (false, true) => match number(first) {
            None => RangeSpec::Full,
            Some(start) if start < len => RangeSpec::Partial { start, end: len - 1 },
            Some(_) => RangeSpec::Unsatisfiable,
        },
        (false, false) => match (number(first), number(last)) {
            (Some(start), Some(end)) if end < start => RangeSpec::Full,
            (Some(start), Some(_)) if start >= len => RangeSpec::Unsatisfiable,
            (Some(start), Some(end)) => RangeSpec::Partial { start, end: end.min(len - 1) },
            _ => RangeSpec::Full,
        },
    }
}

fn serve_range(req: &Req, content_type: &str, data: &[u8]) -> Resp {
    let len = data.len() as u64;
    let base = |status: u16| Resp::new(status).with("Accept-Ranges", "bytes");
    match parse_range(req.header("range"), len) {
        RangeSpec::Full => {
            let mut r = base(200).with("Content-Type", content_type).with("Content-Length", &len.to_string());
            if req.method != "HEAD" {
                r.body = data.to_vec();
            }
            r
        }
        RangeSpec::Partial { start, end } => {
            let part = &data[start as usize..=end as usize];
            let mut r = base(206)
                .with("Content-Type", content_type)
                .with("Content-Range", &format!("bytes {start}-{end}/{len}"))
                .with("Content-Length", &part.len().to_string());
            if req.method != "HEAD" {
                r.body = part.to_vec();
            }
            r
        }
        RangeSpec::Unsatisfiable => base(416).with("Content-Range", &format!("bytes */{len}")).with("Content-Length", "0"),
    }
}

fn main() {
    let data = b"0123456789";
    println!("{:?}", serve_range(&Req::new("GET").with("Range", "bytes=2-4"), "text/plain", data).status);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str, len: u64) -> RangeSpec {
        parse_range(Some(s), len)
    }

    fn part(start: u64, end: u64) -> RangeSpec {
        RangeSpec::Partial { start, end }
    }

    #[test]
    fn the_three_forms() {
        assert_eq!(p("bytes=0-99", 1000), part(0, 99));
        assert_eq!(p("bytes=100-", 1000), part(100, 999), "от 100-го байта до конца");
        assert_eq!(p("bytes=-50", 1000), part(950, 999), "последние 50 байтов");
        assert_eq!(p("bytes=5-5", 10), part(5, 5), "один байт");
    }

    #[test]
    fn ends_are_clipped_to_the_file() {
        assert_eq!(p("bytes=0-99999", 1000), part(0, 999));
        assert_eq!(p("bytes=-5000", 1000), part(0, 999), "суффикс длиннее файла это весь файл");
        assert_eq!(p("bytes=995-1500", 1000), part(995, 999));
    }

    #[test]
    fn out_of_range_is_unsatisfiable() {
        assert_eq!(p("bytes=1000-", 1000), RangeSpec::Unsatisfiable, "файл кончается на байте 999");
        assert_eq!(p("bytes=1000-1010", 1000), RangeSpec::Unsatisfiable);
        assert_eq!(p("bytes=-0", 1000), RangeSpec::Unsatisfiable, "ноль последних байтов");
        assert_eq!(p("bytes=0-", 0), RangeSpec::Unsatisfiable, "в пустом файле выбрать нечего");
        assert_eq!(p("bytes=-5", 0), RangeSpec::Unsatisfiable);
    }

    #[test]
    fn what_we_do_not_understand_is_served_whole() {
        for odd in ["bytes=5-2", "bytes=a-b", "bytes=-", "bytes=", "items=0-5", "bytes 0-5", "bytes=0-5,10-15", "bytes=+1-5", "bytes=1-5-9", ""] {
            assert_eq!(p(odd, 100), RangeSpec::Full, "Range {odd:?} разбирать не берёмся, отдаём файл целиком (так велит стандарт)");
        }
        assert_eq!(parse_range(None, 100), RangeSpec::Full);
    }

    fn data() -> Vec<u8> {
        (0..=255u8).collect()
    }

    #[test]
    fn partial_content_response() {
        let r = serve_range(&Req::new("GET").with("Range", "bytes=10-19"), "application/octet-stream", &data());
        assert_eq!(r.status, 206);
        assert_eq!(r.body, &data()[10..20]);
        assert_eq!(r.header("content-range"), Some("bytes 10-19/256"));
        assert_eq!(r.header("content-length"), Some("10"));
        assert_eq!(r.header("accept-ranges"), Some("bytes"));
        assert_eq!(r.header("content-type"), Some("application/octet-stream"));
        let r = serve_range(&Req::new("GET").with("Range", "bytes=-4"), "application/octet-stream", &data());
        assert_eq!((r.status, r.body.as_slice(), r.header("content-range")), (206, &[252u8, 253, 254, 255][..], Some("bytes 252-255/256")));
    }

    #[test]
    fn whole_file_still_advertises_ranges() {
        let r = serve_range(&Req::new("GET"), "text/plain", b"hello");
        assert_eq!((r.status, r.body.as_slice()), (200, &b"hello"[..]));
        assert_eq!(r.header("accept-ranges"), Some("bytes"), "клиент узнаёт о докачке из ответа на обычный запрос");
        assert_eq!(r.header("content-length"), Some("5"));
        let r = serve_range(&Req::new("GET").with("Range", "bytes=1-2,4-5"), "text/plain", b"hello");
        assert_eq!(r.status, 200, "несколько диапазонов не поддерживаем: отдаём целиком");
    }

    #[test]
    fn unsatisfiable_tells_the_real_size() {
        let r = serve_range(&Req::new("GET").with("Range", "bytes=500-"), "text/plain", b"hello");
        assert_eq!(r.status, 416);
        assert_eq!(r.header("content-range"), Some("bytes */5"), "в 416 Content-Range сообщает настоящий размер");
        assert!(r.body.is_empty());
    }

    #[test]
    fn head_with_range_has_no_body() {
        let r = serve_range(&Req::new("HEAD").with("Range", "bytes=1-3"), "text/plain", b"hello");
        assert_eq!(r.status, 206);
        assert!(r.body.is_empty());
        assert_eq!(r.header("content-length"), Some("3"));
    }
}
