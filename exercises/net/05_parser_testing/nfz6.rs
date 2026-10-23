// Генератор делает валидные входы. Но баги парсеров живут на невалидных: оборванных, с лишним байтом, с
// перепутанными кусками. Мутационный фаззинг берёт хороший вход и портит его по мелочи, много раз.
// TODO: Напиши mutate: 1..3 раунда, в каждом случайная операция: перевернуть бит, заменить байт случайным,
// вставить 1..4 случайных байта, удалить 1..8 подряд, продублировать кусок до 8 байт, вставить кусок другого
// входа из corpus, записать «интересное» значение из INTERESTING. На пустом входе и пустом corpus не паниковать.

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

const INTERESTING: [u8; 8] = [0, 1, 0x7f, 0x80, 0xff, b'\r', b'\n', b' '];

fn mutate(rng: &mut Rng, input: &[u8], corpus: &[Vec<u8>]) -> Vec<u8> {
    todo!()
}

fn main() {
    let mut rng = Rng::new(1);
    println!("{:?}", mutate(&mut rng, b"GET / HTTP/1.1\r\n\r\n", &[]));
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &[u8] = b"GET /a?x=1 HTTP/1.1\r\nHost: h\r\nAccept: */*\r\n\r\n";

    fn run(seed: u64, input: &[u8], corpus: &[Vec<u8>]) -> Vec<u8> {
        mutate(&mut Rng::new(seed), input, corpus)
    }

    #[test]
    fn deterministic_for_a_seed() {
        let corpus = vec![b"other input".to_vec()];
        for seed in 0..100 {
            assert_eq!(run(seed, VALID, &corpus), run(seed, VALID, &corpus), "seed {seed}: мутатор использует что-то кроме переданного Rng");
        }
    }

    #[test]
    fn almost_always_changes_the_input() {
        let changed = (0..2000).filter(|s| run(*s, VALID, &[]) != VALID).count();
        assert!(changed >= 1700, "изменено только {changed} из 2000: мутация, которая ничего не меняет, это пустая трата итерации");
    }

    #[test]
    fn length_goes_up_down_and_stays() {
        let (mut longer, mut shorter, mut same) = (0, 0, 0);
        for s in 0..2000 {
            match run(s, VALID, &[]).len().cmp(&VALID.len()) {
                std::cmp::Ordering::Greater => longer += 1,
                std::cmp::Ordering::Less => shorter += 1,
                std::cmp::Ordering::Equal => same += 1,
            }
        }
        assert!(longer > 100 && shorter > 100 && same > 100, "длина должна и расти, и падать, и сохраняться: больше {longer}, меньше {shorter}, равно {same}");
    }

    #[test]
    fn single_bit_flips_happen() {
        let flips = (0..3000)
            .filter(|s| {
                let out = run(*s, VALID, &[]);
                out.len() == VALID.len() && out.iter().zip(VALID).filter(|(a, b)| a != b).count() == 1 && out.iter().zip(VALID).any(|(a, b)| (a ^ b).count_ones() == 1)
            })
            .count();
        assert!(flips >= 20, "переворот одного бита встретился {flips} раз из 3000: такой операции в мутаторе нет или она слишком редкая");
    }

    #[test]
    fn splices_pieces_of_other_inputs() {
        let corpus = vec![vec![b'Z'; 6]];
        let hits = (0..3000).filter(|s| run(*s, b"abcdefgh", &corpus).contains(&b'Z')).count();
        assert!(hits >= 50, "кусок из corpus попал в результат {hits} раз из 3000");
    }

    #[test]
    fn empty_input_and_empty_corpus_do_not_panic() {
        for s in 0..500 {
            let _ = run(s, b"", &[]);
            let _ = run(s, b"", &[Vec::new()]);
            let _ = run(s, b"x", &[Vec::new()]);
        }
    }

    #[test]
    fn mutations_break_and_keep_the_structure() {
        let req = serialize(&Request { method: "POST".into(), path: "/a".into(), query: vec![], headers: vec![("Host".into(), "h".into())], body: b"hello".to_vec() });
        let (mut ok, mut err) = (0, 0);
        for s in 0..5000 {
            match parse(&run(s, &req, &[])) {
                Ok(_) => ok += 1,
                Err(_) => err += 1,
            }
        }
        assert!(ok >= 300 && err >= 300, "из 5000 мутаций разобрались {ok}, сломались {err}: нужны и те, и другие, иначе фаззер ходит по одним и тем же веткам парсера");
    }
}
