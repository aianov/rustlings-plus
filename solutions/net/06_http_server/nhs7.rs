use std::str::FromStr;

#[derive(Debug, Clone, PartialEq)]
struct Response {
    status: u16,
    body: String,
}

#[derive(Debug, PartialEq)]
struct Params(Vec<(String, String)>);

fn match_path(pattern: &str, path: &str) -> Option<Params> {
    let pattern: Vec<&str> = pattern.split('/').collect();
    let path: Vec<&str> = path.split('/').collect();
    if pattern.len() != path.len() {
        return None;
    }
    let mut found = Vec::new();
    for (expected, actual) in pattern.into_iter().zip(path) {
        match expected.strip_prefix(':') {
            Some(_) if actual.is_empty() => return None,
            Some(name) => found.push((name.to_string(), actual.to_string())),
            None if expected == actual => {}
            None => return None,
        }
    }
    Some(Params(found))
}

impl Params {
    fn get(&self, name: &str) -> Option<&str> {
        self.0.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
    }

    fn parse<T: FromStr>(&self, name: &str) -> Result<T, Response> {
        let Some(raw) = self.get(name) else {
            return Err(Response { status: 500, body: format!("route has no parameter '{name}'") });
        };
        raw.parse().map_err(|_| Response { status: 400, body: format!("bad parameter '{name}': {raw:?}") })
    }
}

fn show_user(params: &Params) -> Response {
    match params.parse::<u32>("id") {
        Ok(id) => Response { status: 200, body: format!("user {id}") },
        Err(response) => response,
    }
}

fn main() {
    let params = match_path("/users/:id", "/users/42").expect("маршрут подходит");
    println!("{:?}", show_user(&params));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(p: Option<Params>) -> Vec<(String, String)> {
        p.expect("маршрут должен подойти").0
    }

    #[test]
    fn captures_segments() {
        assert_eq!(pairs(match_path("/users/:id", "/users/42")), vec![("id".to_string(), "42".to_string())]);
        assert_eq!(
            pairs(match_path("/posts/:user/comments/:n", "/posts/ада/comments/3")),
            vec![("user".to_string(), "ада".to_string()), ("n".to_string(), "3".to_string())]
        );
        assert_eq!(pairs(match_path("/", "/")), vec![]);
        assert_eq!(pairs(match_path("/health", "/health")), vec![]);
    }

    #[test]
    fn rejects_other_shapes() {
        for (pattern, path) in [
            ("/users/:id", "/users"),
            ("/users/:id", "/users/42/"),
            ("/users/:id", "/users/42/posts"),
            ("/users/:id", "/users/"),
            ("/users/:id", "//42"),
            ("/users/:id", "/people/42"),
            ("/users/:id", "/Users/42"),
            ("/", "/x"),
        ] {
            assert!(match_path(pattern, path).is_none(), "{pattern} не должен подходить к {path:?}");
        }
    }

    #[test]
    fn user_id_must_be_a_number() {
        let ok = show_user(&match_path("/users/:id", "/users/42").unwrap());
        assert_eq!((ok.status, ok.body.as_str()), (200, "user 42"));
        for bad in ["abc", "-1", "4294967296", "1.5", "4 2", "мария"] {
            let params = match_path("/users/:id", &format!("/users/{bad}")).unwrap();
            let reply = show_user(&params);
            assert_eq!(reply.status, 400, "{bad:?} не u32, а это вина клиента");
            assert!(reply.body.contains("id"), "в сообщении должно быть имя параметра: {:?}", reply.body);
        }
    }

    #[test]
    fn missing_parameter_is_server_bug() {
        let params = match_path("/users/:name", "/users/ada").unwrap();
        assert_eq!(show_user(&params).status, 500, "в шаблоне нет :id, это ошибка программиста, а не клиента");
        assert_eq!(params.parse::<String>("name").unwrap(), "ada");
    }
}
