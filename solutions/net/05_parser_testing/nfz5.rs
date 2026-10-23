#[derive(Debug, Clone)]
struct Rng {
    state: u64,
}

#[allow(dead_code)]
impl Rng {
    fn new(seed: u64) -> Rng {
        Rng { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: u64) -> u64 {
        assert!(n > 0, "below(0) не имеет смысла");
        let limit = u64::MAX - u64::MAX % n;
        loop {
            let x = self.next_u64();
            if x < limit {
                return x % n;
            }
        }
    }

    fn range(&mut self, lo: u64, hi: u64) -> u64 {
        lo + self.below(hi - lo)
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len() as u64) as usize]
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Request {
    method: String,
    path: String,
    query: Vec<(String, String)>,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

fn serialize(r: &Request) -> Vec<u8> {
    let mut target = r.path.clone();
    for (i, (k, v)) in r.query.iter().enumerate() {
        target.push(if i == 0 { '?' } else { '&' });
        target.push_str(&format!("{k}={v}"));
    }
    let mut out = format!("{} {} HTTP/1.1\r\n", r.method, target).into_bytes();
    for (k, v) in &r.headers {
        out.extend_from_slice(format!("{k}: {v}\r\n").as_bytes());
    }
    if !r.body.is_empty() {
        out.extend_from_slice(format!("Content-Length: {}\r\n", r.body.len()).as_bytes());
    }
    out.extend_from_slice(b"\r\n");
    out.extend_from_slice(&r.body);
    out
}

fn parse(buf: &[u8]) -> Result<Request, String> {
    let end = buf.windows(4).position(|w| w == b"\r\n\r\n").ok_or("нет конца заголовков")?;
    let head = std::str::from_utf8(&buf[..end]).map_err(|_| "голова не UTF-8")?;
    let mut lines = head.split("\r\n");
    let first = lines.next().ok_or("пусто")?;
    let parts: Vec<&str> = first.split(' ').collect();
    if parts.len() != 3 {
        return Err("строка запроса".into());
    }
    let (path, query) = match parts[1].split_once('?') {
        Some((p, q)) => (p, q.split('&').filter_map(|kv| kv.split_once('=')).map(|(k, v)| (k.to_string(), v.to_string())).collect()),
        None => (parts[1], Vec::new()),
    };
    let mut headers = Vec::new();
    let mut length = 0usize;
    for line in lines {
        let (k, v) = line.split_once(": ").ok_or("заголовок")?;
        if k.eq_ignore_ascii_case("content-length") {
            length = v.parse().map_err(|_| "Content-Length")?;
        } else {
            headers.push((k.to_string(), v.to_string()));
        }
    }
    let body = buf.get(end + 4..end + 4 + length).ok_or("тело короче длины")?.to_vec();
    Ok(Request { method: parts[0].to_string(), path: path.to_string(), query, headers, body })
}

fn gen_request(rng: &mut Rng) -> Request {
    const METHODS: [&str; 5] = ["GET", "POST", "PUT", "DELETE", "HEAD"];
    const NAMES: [&str; 5] = ["Host", "Accept", "X-Trace", "User-Agent", "Accept-Language"];
    const ALNUM: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    fn word(rng: &mut Rng, min: u64, max: u64) -> String {
        (0..rng.range(min, max + 1)).map(|_| *rng.pick(ALNUM) as char).collect()
    }
    let method = rng.pick(&METHODS).to_string();
    let mut path = String::new();
    for _ in 0..rng.below(4) {
        path.push('/');
        path.push_str(&word(rng, 1, 8));
    }
    if path.is_empty() {
        path.push('/');
    }
    let query = (0..rng.below(3)).map(|_| (word(rng, 1, 5), word(rng, 0, 5))).collect();
    let headers = (0..rng.below(4))
        .map(|_| {
            let value: String = (0..rng.range(1, 13)).map(|_| (0x21 + rng.below(0x5e) as u8) as char).collect();
            (rng.pick(&NAMES).to_string(), value)
        })
        .collect();
    let body = if (method == "POST" || method == "PUT") && rng.chance(70) { (0..rng.below(48)).map(|_| rng.next_u64() as u8).collect() } else { Vec::new() };
    Request { method, path, query, headers, body }
}

#[derive(Debug)]
struct Failure {
    seed: u64,
    message: String,
}

fn run_property(cases: u64, base_seed: u64, prop: impl Fn(&mut Rng) -> Result<(), String>) -> Result<(), Failure> {
    for i in 0..cases {
        let seed = base_seed.wrapping_add(i);
        if let Err(message) = prop(&mut Rng::new(seed)) {
            return Err(Failure { seed, message });
        }
    }
    Ok(())
}

fn roundtrip(parser: impl Fn(&[u8]) -> Result<Request, String>) -> impl Fn(&mut Rng) -> Result<(), String> {
    move |rng| {
        let request = gen_request(rng);
        let bytes = serialize(&request);
        match parser(&bytes) {
            Ok(back) if back == request => Ok(()),
            other => Err(format!("запрос {request:?} после сериализации и разбора стал {other:?}")),
        }
    }
}

fn main() {
    println!("{:?}", run_property(100, 0, roundtrip(parse)).is_ok());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn parse_without_duplicate_headers(buf: &[u8]) -> Result<Request, String> {
        let mut r = parse(buf)?;
        let mut seen = std::collections::HashSet::new();
        r.headers.retain(|(k, _)| seen.insert(k.clone()));
        Ok(r)
    }

    #[test]
    fn runs_exactly_the_requested_cases() {
        let runs = Cell::new(0);
        let res = run_property(120, 5, |_| {
            runs.set(runs.get() + 1);
            Ok(())
        });
        assert!(res.is_ok());
        assert_eq!(runs.get(), 120, "прогнано {} случаев вместо 120", runs.get());
    }

    #[test]
    fn stops_at_the_first_failure_and_reports_its_seed() {
        let fails = |rng: &mut Rng| if rng.next_u64() % 10 == 3 { Err("плохой случай".to_string()) } else { Ok(()) };
        let expected = (1000..).find(|s| fails(&mut Rng::new(*s)).is_err()).unwrap();
        let runs = Cell::new(0);
        let failure = run_property(10_000, 1000, |rng| {
            runs.set(runs.get() + 1);
            fails(rng)
        })
        .expect_err("свойство нарушается примерно в каждом десятом случае");
        assert_eq!(failure.seed, expected, "seed первого сбоя это base_seed + номер случая");
        assert_eq!(failure.message, "плохой случай");
        assert_eq!(runs.get(), expected - 1000 + 1, "после первого сбоя прогон должен остановиться");
        assert!(fails(&mut Rng::new(failure.seed)).is_err(), "seed из отчёта обязан воспроизводить сбой");
    }

    #[test]
    fn correct_parser_satisfies_roundtrip() {
        let res = run_property(500, 0, roundtrip(parse));
        assert!(res.is_ok(), "верный парсер не должен падать: {res:?}");
    }

    #[test]
    fn broken_parser_is_caught_with_a_readable_report() {
        let failure = run_property(500, 0, roundtrip(parse_without_duplicate_headers)).expect_err("парсер теряет повторяющиеся заголовки, свойство должно это поймать");
        let request = gen_request(&mut Rng::new(failure.seed));
        assert!(request.headers.iter().any(|(k, _)| request.headers.iter().filter(|(k2, _)| k2 == k).count() > 1), "по seed {} запроса с дублями нет", failure.seed);
        assert!(failure.message.contains(&request.path) && failure.message.contains("headers"), "в сообщении должен быть сам запрос, чтобы было видно, на чём упало: {:?}", failure.message);
    }
}
