
#[derive(Debug, Clone)]
pub struct Principal {
    pub user: String,
    pub role: String,
}

pub struct Request {
    pub path: String,
    pub headers: Vec<(String, String)>,
}

#[derive(Debug, PartialEq)]
pub struct Response {
    pub status: u16,
    pub body: String,
}

#[derive(Debug, PartialEq)]
pub enum CheckError {
    Expired,
    Invalid,
}

pub trait TokenCheck {
    fn check(&self, token: &str) -> Result<Principal, CheckError>;
}

type Public = Box<dyn Fn(&Request) -> Response>;
type Protected = Box<dyn Fn(&Principal, &Request) -> Response>;

pub struct Router<C: TokenCheck> {
    checker: C,
    public: Vec<(String, Public)>,
    protected: Vec<(String, Protected)>,
}

fn unauthorized(code: &str) -> Response {
    Response { status: 401, body: format!("{{\"error\":\"{code}\"}}") }
}

impl<C: TokenCheck> Router<C> {
    pub fn new(checker: C) -> Router<C> {
        Router { checker, public: Vec::new(), protected: Vec::new() }
    }

    pub fn public(mut self, path: &str, handler: impl Fn(&Request) -> Response + 'static) -> Router<C> {
        self.public.push((path.to_string(), Box::new(handler)));
        self
    }

    pub fn protected(mut self, path: &str, handler: impl Fn(&Principal, &Request) -> Response + 'static) -> Router<C> {
        self.protected.push((path.to_string(), Box::new(handler)));
        self
    }

    fn credentials(&self, request: &Request) -> Result<Option<Principal>, Response> {
        let mut found = request.headers.iter().filter(|(name, _)| name.eq_ignore_ascii_case("authorization"));
        let Some((_, value)) = found.next() else {
            return Ok(None);
        };
        if found.next().is_some() {
            return Err(unauthorized("auth.malformed"));
        }
        let token = match value.split_once(' ') {
            Some((scheme, token)) if scheme.eq_ignore_ascii_case("bearer") && !token.is_empty() && !token.contains(' ') => token,
            _ => return Err(unauthorized("auth.malformed")),
        };
        match self.checker.check(token) {
            Ok(principal) => Ok(Some(principal)),
            Err(CheckError::Expired) => Err(unauthorized("auth.expired")),
            Err(CheckError::Invalid) => Err(unauthorized("auth.invalid")),
        }
    }

    pub fn dispatch(&self, request: &Request) -> Response {
        let principal = match self.credentials(request) {
            Ok(principal) => principal,
            Err(response) => return response,
        };
        if let Some((_, handler)) = self.public.iter().find(|(path, _)| *path == request.path) {
            return handler(request);
        }
        if let Some((_, handler)) = self.protected.iter().find(|(path, _)| *path == request.path) {
            return match principal {
                Some(principal) => handler(&principal, request),
                None => unauthorized("auth.missing"),
            };
        }
        Response { status: 404, body: String::new() }
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    struct Fake;

    impl TokenCheck for Fake {
        fn check(&self, token: &str) -> Result<Principal, CheckError> {
            match token {
                "good" => Ok(Principal { user: "alice".to_string(), role: "user".to_string() }),
                "old" => Err(CheckError::Expired),
                _ => Err(CheckError::Invalid),
            }
        }
    }

    fn request(path: &str, headers: &[(&str, &str)]) -> Request {
        Request { path: path.to_string(), headers: headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect() }
    }

    fn router() -> (Router<Fake>, Rc<Cell<u32>>, Rc<Cell<u32>>) {
        let public_calls = Rc::new(Cell::new(0));
        let protected_calls = Rc::new(Cell::new(0));
        let (p, q) = (public_calls.clone(), protected_calls.clone());
        let router = Router::new(Fake)
            .public("/feed", move |_| {
                p.set(p.get() + 1);
                Response { status: 200, body: "feed".to_string() }
            })
            .protected("/me", move |who, _| {
                q.set(q.get() + 1);
                Response { status: 200, body: who.user.clone() }
            });
        (router, public_calls, protected_calls)
    }

    #[test]
    fn protected_route_without_a_token_is_401_and_the_handler_is_not_called() {
        let (router, _, calls) = router();
        let response = router.dispatch(&request("/me", &[]));
        assert_eq!(response.status, 401, "без токена на защищённый маршрут нельзя");
        assert!(response.body.contains("auth.missing"), "код отсутствия токена: {}", response.body);
        assert_eq!(calls.get(), 0, "обработчик вызываться не должен");
    }

    #[test]
    fn valid_token_reaches_the_handler_with_the_principal() {
        let (router, _, calls) = router();
        let response = router.dispatch(&request("/me", &[("Authorization", "Bearer good")]));
        assert_eq!(response, Response { status: 200, body: "alice".to_string() }, "обработчик получил пользователя из токена");
        assert_eq!(calls.get(), 1, "один вызов");
    }

    #[test]
    fn header_name_and_scheme_are_case_insensitive() {
        let (router, _, _) = router();
        for (name, value) in [("authorization", "Bearer good"), ("AUTHORIZATION", "bearer good"), ("Authorization", "BEARER good")] {
            assert_eq!(router.dispatch(&request("/me", &[(name, value)])).status, 200, "{name}: {value}");
        }
    }

    #[test]
    fn malformed_headers_are_rejected() {
        let (router, _, calls) = router();
        for value in ["good", "Bearer", "Bearer ", "Bearer  good", "Bearer good extra", "Basic Z29vZA==", "Token good", " Bearer good"] {
            let response = router.dispatch(&request("/me", &[("Authorization", value)]));
            assert_eq!(response.status, 401, "{value:?} не годится");
            assert!(response.body.contains("auth.malformed"), "{value:?}: код {}", response.body);
        }
        let twice = router.dispatch(&request("/me", &[("Authorization", "Bearer good"), ("authorization", "Bearer good")]));
        assert_eq!(twice.status, 401, "два заголовка Authorization это неоднозначность");
        assert_eq!(calls.get(), 0, "ни один сломанный запрос не дошёл до обработчика");
    }

    #[test]
    fn expired_and_invalid_tokens_have_different_codes() {
        let (router, _, _) = router();
        let expired = router.dispatch(&request("/me", &[("Authorization", "Bearer old")]));
        assert!(expired.status == 401 && expired.body.contains("auth.expired"), "просроченный: клиент должен обновить токен: {expired:?}");
        let invalid = router.dispatch(&request("/me", &[("Authorization", "Bearer junk")]));
        assert!(invalid.status == 401 && invalid.body.contains("auth.invalid"), "неверный: {invalid:?}");
    }

    #[test]
    fn public_route_works_without_a_token_but_not_with_a_bad_one() {
        let (router, calls, _) = router();
        assert_eq!(router.dispatch(&request("/feed", &[])).status, 200, "аноним читает ленту");
        assert_eq!(router.dispatch(&request("/feed", &[("Authorization", "Bearer good")])).status, 200, "и вошедший тоже");
        let bad = router.dispatch(&request("/feed", &[("Authorization", "Bearer junk")]));
        assert_eq!(bad.status, 401, "испорченный токен на публичном маршруте это не анонимный запрос, а ошибка клиента, которую стоит показать");
        assert_eq!(calls.get(), 2, "обработчик вызван только дважды");
    }

    #[test]
    fn unknown_route_is_404() {
        let (router, _, _) = router();
        assert_eq!(router.dispatch(&request("/nope", &[])).status, 404, "нет такого маршрута");
    }
}
