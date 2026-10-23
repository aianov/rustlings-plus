// Профиль пользователя запрашивают каждые несколько секунд, а меняется он раз в неделю. Качать каждый раз 4 КБ
// впустую обидно. HTTP придумал для этого валидатор: сервер присылает ETag («отпечаток версии»), клиент в следующий раз
// спрашивает «у меня версия X, она ещё актуальна?», и если да, сервер отвечает 304 без тела.
// TODO: Допиши etag_matches (разбор If-None-Match: список, слабые теги W/"…", звёздочка) и conditional_get.
// Свежий клиент получает 304 с ETag и Cache-Control: no-cache, остальные 200 с телом, ETag и длиной. HEAD без тела.

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

fn etag(body: &[u8]) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in body {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("\"{hash:016x}\"")
}

fn etag_matches(if_none_match: &str, etag: &str) -> bool {
    let _ = (if_none_match, etag);
    todo!()
}

fn conditional_get(req: &Req, content_type: &str, body: &[u8]) -> Resp {
    let _ = (req, content_type, body);
    todo!()
}

fn main() {
    let req = Req::new("GET");
    println!("{:?}", conditional_get(&req, "text/plain", b"hello").header("etag"));
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: &[u8] = b"{\"name\":\"Ada\"}";

    #[test]
    fn first_visit_gets_the_body_and_a_validator() {
        let r = conditional_get(&Req::new("GET"), "application/json", BODY);
        assert_eq!(r.status, 200);
        assert_eq!(r.body, BODY);
        assert_eq!(r.header("etag"), Some(etag(BODY).as_str()));
        assert_eq!(r.header("content-type"), Some("application/json"));
        assert_eq!(r.header("content-length"), Some("14"));
        assert_eq!(r.header("cache-control"), Some("no-cache"), "no-cache значит «можно хранить, но перед использованием спроси»");
    }

    #[test]
    fn matching_validator_gets_304_without_a_body() {
        let tag = etag(BODY);
        let r = conditional_get(&Req::new("GET").with("If-None-Match", &tag), "application/json", BODY);
        assert_eq!(r.status, 304);
        assert!(r.body.is_empty(), "у 304 тела нет: смысл в том, что оно не едет по сети");
        assert_eq!(r.header("etag"), Some(tag.as_str()), "304 повторяет ETag");
        assert_eq!(r.header("cache-control"), Some("no-cache"));
        assert!(r.header("content-length").is_none(), "длину тела, которого нет, не пишем");
    }

    #[test]
    fn changed_body_means_changed_etag() {
        let old = etag(BODY);
        let r = conditional_get(&Req::new("GET").with("If-None-Match", &old), "application/json", b"{\"name\":\"Bob\"}");
        assert_eq!(r.status, 200, "содержимое изменилось: старая копия клиента недействительна");
        assert_ne!(r.header("etag"), Some(old.as_str()));
        assert_eq!(etag(BODY), etag(BODY), "etag детерминирован");
    }

    #[test]
    fn head_has_headers_but_no_body() {
        let r = conditional_get(&Req::new("HEAD"), "application/json", BODY);
        assert_eq!(r.status, 200);
        assert!(r.body.is_empty());
        assert_eq!(r.header("content-length"), Some("14"), "у HEAD длина настоящая");
        let tag = etag(BODY);
        let r = conditional_get(&Req::new("HEAD").with("If-None-Match", &tag), "application/json", BODY);
        assert_eq!(r.status, 304);
    }

    #[test]
    fn if_none_match_can_be_a_list_a_weak_tag_or_a_star() {
        let tag = etag(BODY);
        let list = format!("\"nope\", {tag} , \"other\"");
        assert!(etag_matches(&list, &tag), "нужный etag среди нескольких");
        assert!(etag_matches(&format!("W/{tag}"), &tag), "слабое сравнение: W/ отбрасывается");
        assert!(etag_matches("*", &tag), "* значит «любая версия»");
        assert!(etag_matches(&format!("\"a,b\", {tag}"), &tag), "запятая внутри кавычек не разделитель");
        assert!(etag_matches("\"a,b\"", "\"a,b\""), "запятая внутри самого etag");
        assert!(!etag_matches("\"nope\"", &tag));
        assert!(!etag_matches("", &tag));
        assert!(!etag_matches(&tag[1..tag.len() - 1], &tag), "без кавычек это не etag");
        assert!(!etag_matches("W/", &tag));
        assert!(!etag_matches(&format!("{tag}x"), &tag), "частичное совпадение не считается");
    }
}
