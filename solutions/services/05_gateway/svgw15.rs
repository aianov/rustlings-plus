
const UNTRUSTED: [&str; 5] = ["x-forwarded-for", "x-real-ip", "forwarded", "x-forwarded-proto", "x-forwarded-host"];

pub fn forward_headers(client: &[(String, String)], peer_ip: &str) -> Vec<(String, String)> {
    let mut headers: Vec<(String, String)> = client.iter().filter(|(name, _)| !UNTRUSTED.contains(&name.to_ascii_lowercase().as_str())).cloned().collect();
    headers.push(("x-forwarded-for".to_string(), peer_ip.to_string()));
    headers.push(("x-real-ip".to_string(), peer_ip.to_string()));
    headers.push(("x-forwarded-proto".to_string(), "https".to_string()));
    headers
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(items: &[(&str, &str)]) -> Vec<(String, String)> {
        items.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    fn get<'a>(headers: &'a [(String, String)], name: &str) -> Vec<&'a str> {
        headers.iter().filter(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str()).collect()
    }

    fn upstream_limiter_key(headers: &[(String, String)]) -> String {
        get(headers, "x-forwarded-for").first().and_then(|v| v.split(',').next()).unwrap_or("").trim().to_string()
    }

    #[test]
    fn the_address_is_the_tcp_peer_not_what_the_client_claims() {
        let client = pairs(&[("X-Forwarded-For", "10.0.0.1"), ("Content-Type", "application/json")]);
        let out = forward_headers(&client, "203.0.113.9");
        assert_eq!(get(&out, "x-forwarded-for"), vec!["203.0.113.9"], "ровно один x-forwarded-for и это настоящий адрес");
        assert_eq!(upstream_limiter_key(&out), "203.0.113.9", "апстрим, берущий первый элемент, видит настоящий адрес");
    }

    #[test]
    fn the_whole_family_of_address_headers_is_replaced() {
        let client = pairs(&[("x-real-ip", "6.6.6.6"), ("Forwarded", "for=6.6.6.6;proto=http"), ("X-Forwarded-Proto", "http"), ("x-forwarded-host", "evil.example")]);
        let out = forward_headers(&client, "2001:db8::1");
        assert_eq!(get(&out, "x-real-ip"), vec!["2001:db8::1"], "x-real-ip");
        assert_eq!(get(&out, "x-forwarded-proto"), vec!["https"], "схема известна шлюзу, а не клиенту");
        assert!(get(&out, "forwarded").is_empty(), "стандартный Forwarded от клиента тоже не передаётся");
        assert!(get(&out, "x-forwarded-host").is_empty(), "и подмена хоста");
    }

    #[test]
    fn repeated_spoofed_headers_all_disappear() {
        let client = pairs(&[("x-forwarded-for", "1.1.1.1"), ("X-FORWARDED-FOR", "2.2.2.2")]);
        let out = forward_headers(&client, "203.0.113.9");
        assert_eq!(get(&out, "x-forwarded-for"), vec!["203.0.113.9"], "оба заголовка клиента выброшены");
    }

    #[test]
    fn ordinary_headers_survive_in_order() {
        let client = pairs(&[("Authorization", "Bearer t"), ("X-Forwarded-For", "1.2.3.4"), ("Accept", "*/*")]);
        let out = forward_headers(&client, "203.0.113.9");
        assert_eq!(out[0], ("Authorization".to_string(), "Bearer t".to_string()), "обычные заголовки на месте");
        assert_eq!(out[1], ("Accept".to_string(), "*/*".to_string()), "и порядок сохранён");
    }
}
