// Роутер работает, пока маршруты регистрируют в удачном порядке. Но стоит поставить "/users/:id" раньше
// "/users/me", и профиль "me" уходит в обработчик числового id, получает 400 "bad parameter" и все
// удивляются. Баг сидит в find: он берёт первый подошедший маршрут.
// TODO: Почини find: результат не должен зависеть от порядка регистрации. Из подходящих выигрывает самый
// конкретный: сравнивай сегменты слева направо, где статичный сегмент сильнее параметра.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum Method {
    Get,
    Post,
}

struct Route {
    method: Method,
    pattern: &'static str,
    name: &'static str,
}

#[derive(Default)]
struct Router {
    routes: Vec<Route>,
}

fn matches(pattern: &str, path: &str) -> Option<Vec<(String, String)>> {
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
    Some(found)
}

impl Router {
    fn route(mut self, method: Method, pattern: &'static str, name: &'static str) -> Router {
        self.routes.push(Route { method, pattern, name });
        self
    }

    fn find(&self, method: Method, path: &str) -> Option<(&'static str, Vec<(String, String)>)> {
        self.routes
            .iter()
            .filter(|route| route.method == method)
            .find_map(|route| matches(route.pattern, path).map(|params| (route.name, params)))
    }
}

fn main() {
    let router = Router::default()
        .route(Method::Get, "/users/:id", "show_user")
        .route(Method::Get, "/users/me", "show_me");
    println!("{:?}", router.find(Method::Get, "/users/me"));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(r: &Router, method: Method, path: &str) -> Option<&'static str> {
        r.find(method, path).map(|(n, _)| n)
    }

    #[test]
    fn static_beats_param_in_any_order() {
        let a = Router::default().route(Method::Get, "/users/me", "me").route(Method::Get, "/users/:id", "by_id");
        let b = Router::default().route(Method::Get, "/users/:id", "by_id").route(Method::Get, "/users/me", "me");
        for r in [&a, &b] {
            assert_eq!(name(r, Method::Get, "/users/me"), Some("me"), "статичный сегмент должен побеждать параметр, как бы ни шла регистрация");
            assert_eq!(name(r, Method::Get, "/users/42"), Some("by_id"));
        }
    }

    #[test]
    fn leftmost_static_wins() {
        let a = Router::default().route(Method::Get, "/a/:x/c", "x_c").route(Method::Get, "/a/b/:y", "b_y");
        let b = Router::default().route(Method::Get, "/a/b/:y", "b_y").route(Method::Get, "/a/:x/c", "x_c");
        for r in [&a, &b] {
            assert_eq!(name(r, Method::Get, "/a/b/c"), Some("b_y"), "при споре выигрывает маршрут со статичным сегментом левее");
            assert_eq!(name(r, Method::Get, "/a/z/c"), Some("x_c"));
            assert_eq!(name(r, Method::Get, "/a/b/z"), Some("b_y"));
        }
    }

    #[test]
    fn params_follow_the_chosen_route() {
        let r = Router::default().route(Method::Get, "/files/:name", "file").route(Method::Get, "/files/:dir/:name", "nested");
        assert_eq!(r.find(Method::Get, "/files/a"), Some(("file", vec![("name".to_string(), "a".to_string())])));
        let (n, params) = r.find(Method::Get, "/files/d/a").unwrap();
        assert_eq!((n, params.len()), ("nested", 2));
    }

    #[test]
    fn method_still_matters() {
        let r = Router::default().route(Method::Post, "/users/me", "post_me").route(Method::Get, "/users/:id", "by_id");
        assert_eq!(name(&r, Method::Get, "/users/me"), Some("by_id"), "POST-маршрут не участвует в выборе для GET");
        assert_eq!(name(&r, Method::Post, "/users/7"), None);
    }
}
