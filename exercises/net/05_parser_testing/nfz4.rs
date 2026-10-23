// Тестовые запросы, придуманные вручную, повторяют фантазию автора. Генератор делает тысячи разных, но
// только валидных: иначе вместо поиска багов в парсере мы будем проверять, что он ругается на мусор.
// Правила: метод из пяти (GET, POST, PUT, DELETE, HEAD); путь из 0..3 сегментов [a-z0-9]{1,8} (пустой путь
// это "/"); 0..2 параметра запроса (ключ [a-z0-9]{1,5}, значение [a-z0-9]{0,5}); 0..3 заголовка из набора Host, Accept,
// X-Trace, User-Agent, Accept-Language (имена могут повторяться), значение из 1..12 видимых ASCII (0x21..0x7e,
// без пробела); тело только у POST и PUT, в 70% случаев, 0..47 произвольных байтов.
// TODO: Напиши gen_request по этим правилам, только через Rng: тот же seed даёт тот же запрос.

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
    todo!()
}

fn main() {
    let r = gen_request(&mut Rng::new(1));
    println!("{}", String::from_utf8_lossy(&serialize(&r)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn many() -> Vec<Request> {
        (0..400).map(|seed| gen_request(&mut Rng::new(seed))).collect()
    }

    #[test]
    fn deterministic_per_seed() {
        for seed in 0..50 {
            assert_eq!(gen_request(&mut Rng::new(seed)), gen_request(&mut Rng::new(seed)), "seed {seed}: генератор использует что-то кроме Rng");
        }
    }

    #[test]
    fn generated_requests_roundtrip_through_the_parser() {
        for seed in 0..400 {
            let req = gen_request(&mut Rng::new(seed));
            let bytes = serialize(&req);
            let back = parse(&bytes).unwrap_or_else(|e| panic!("seed {seed}: парсер отверг валидный запрос ({e}): {:?}", String::from_utf8_lossy(&bytes)));
            assert_eq!(back, req, "seed {seed}");
        }
    }

    #[test]
    fn rules_are_respected() {
        let ok_word = |s: &str| s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit());
        for (seed, r) in many().iter().enumerate() {
            assert!(["GET", "POST", "PUT", "DELETE", "HEAD"].contains(&r.method.as_str()), "seed {seed}: метод {}", r.method);
            assert!(r.path.starts_with('/'), "seed {seed}: путь {:?} должен начинаться с '/'", r.path);
            let segments: Vec<&str> = r.path.split('/').skip(1).filter(|s| !s.is_empty()).collect();
            assert!(segments.len() <= 3 && segments.iter().all(|s| (1..=8).contains(&s.len()) && ok_word(s)), "seed {seed}: путь {:?}", r.path);
            assert!(r.query.len() <= 2 && r.query.iter().all(|(k, v)| (1..=5).contains(&k.len()) && ok_word(k) && v.len() <= 5 && ok_word(v)), "seed {seed}: запрос {:?}", r.query);
            assert!(r.headers.len() <= 3, "seed {seed}: заголовков {}", r.headers.len());
            for (k, v) in &r.headers {
                assert!(["Host", "Accept", "X-Trace", "User-Agent", "Accept-Language"].contains(&k.as_str()), "seed {seed}: имя {k:?}");
                assert!((1..=12).contains(&v.len()) && v.bytes().all(|b| (0x21..=0x7e).contains(&b)), "seed {seed}: значение {v:?}");
            }
            assert!(r.body.len() < 48, "seed {seed}: тело {} байтов", r.body.len());
            if r.method != "POST" && r.method != "PUT" {
                assert!(r.body.is_empty(), "seed {seed}: у {} тела быть не должно", r.method);
            }
        }
    }

    #[test]
    fn the_generator_is_varied() {
        let reqs = many();
        let distinct: HashSet<Vec<u8>> = reqs.iter().map(serialize).collect();
        assert!(distinct.len() > 350, "всего {} разных запросов из 400: генератор слишком однообразен", distinct.len());
        let methods: HashSet<&str> = reqs.iter().map(|r| r.method.as_str()).collect();
        assert_eq!(methods.len(), 5, "встретились только методы {methods:?}");
        assert!(reqs.iter().any(|r| !r.body.is_empty()), "нет ни одного запроса с телом");
        assert!(reqs.iter().any(|r| r.method == "POST" && r.body.is_empty()), "POST без тела тоже бывает");
        assert!(reqs.iter().any(|r| r.headers.iter().enumerate().any(|(i, (k, _))| r.headers[..i].iter().any(|(k2, _)| k2 == k))), "нет запросов с повторяющимся именем заголовка");
        assert!(reqs.iter().any(|r| r.path == "/"), "нет корневого пути");
        assert!(reqs.iter().any(|r| !r.query.is_empty()));
    }
}
