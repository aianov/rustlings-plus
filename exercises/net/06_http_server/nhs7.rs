// Маршрут "/users/:id" подходит для "/users/42" и "/users/мария", и обработчику нужно достать из пути
// именно это "42". Параметры приходят строками, а нужны числа: пользователь вводит что угодно, и
// "/users/abc" должен получить 400 с понятным текстом, а не панику и не 404.
// TODO: Напиши match_path (сегменты через '/', ":имя" захватывает непустой сегмент, остальное сравнивается
// точно, число сегментов обязано совпасть), Params::parse и show_user (200 "user <id>" для u32).
// Параметр, которого нет в шаблоне, ошибка программиста: 500. Мусор от пользователя: 400 с именем параметра.

use std::str::FromStr;

#[derive(Debug, Clone, PartialEq)]
struct Response {
    status: u16,
    body: String,
}

#[derive(Debug, PartialEq)]
struct Params(Vec<(String, String)>);

fn match_path(pattern: &str, path: &str) -> Option<Params> {
    todo!()
}

impl Params {
    fn get(&self, name: &str) -> Option<&str> {
        self.0.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
    }

    fn parse<T: FromStr>(&self, name: &str) -> Result<T, Response> {
        todo!()
    }
}

fn show_user(params: &Params) -> Response {
    todo!()
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
