
#[derive(Debug, Clone, PartialEq)]
pub struct GwError {
    pub code: &'static str,
    pub message: String,
}

impl GwError {
    pub fn new(code: &'static str, message: &str) -> GwError {
        GwError { code, message: message.to_string() }
    }
}
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Principal {
    pub user_id: String,
    pub role: String,
}

#[derive(Debug, PartialEq)]
pub enum CheckError {
    Invalid,
    Expired,
}

pub trait TokenCheck: Send + Sync {
    fn validate(&self, token: &str) -> Result<(Principal, u64), CheckError>;
}

pub trait Clock: Send + Sync {
    fn now(&self) -> u64;
}

pub struct ManualClock(std::sync::atomic::AtomicU64);

impl ManualClock {
    pub fn new(start: u64) -> ManualClock {
        ManualClock(std::sync::atomic::AtomicU64::new(start))
    }

    pub fn set(&self, now: u64) {
        self.0.store(now, std::sync::atomic::Ordering::SeqCst);
    }
}

impl Clock for ManualClock {
    fn now(&self) -> u64 {
        self.0.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[derive(Debug)]
pub struct Connection {
    principal: Option<Principal>,
    exp: Option<u64>,
}

impl Connection {
    pub fn principal(&self) -> Option<&Principal> {
        self.principal.as_ref()
    }

    pub fn exp(&self) -> Option<u64> {
        self.exp
    }

    pub fn open(headers: &[(String, String)], query: &str, checker: &dyn TokenCheck) -> Result<Connection, GwError> {
        let from_header = headers.iter().find(|(name, _)| name.eq_ignore_ascii_case("authorization")).map(|(_, value)| {
            value.split_once(' ').filter(|(scheme, token)| scheme.eq_ignore_ascii_case("bearer") && !token.is_empty()).map(|(_, token)| token)
        });
        let from_query: Vec<&str> = query.split('&').filter_map(|pair| pair.strip_prefix("token=")).collect();
        let token = match (from_header, from_query.as_slice()) {
            (Some(None), _) => return Err(GwError::new("unauthenticated", "заголовок Authorization не разобрать")),
            (Some(Some(token)), _) => Some(token),
            (None, []) => None,
            (None, [token]) => Some(*token),
            (None, _) => return Err(GwError::new("bad_request", "токен передан несколько раз")),
        };
        let Some(token) = token else {
            return Ok(Connection { principal: None, exp: None });
        };
        match checker.validate(token) {
            Ok((principal, exp)) => Ok(Connection { principal: Some(principal), exp: Some(exp) }),
            Err(CheckError::Expired) => Err(GwError::new("token_expired", "токен просрочен")),
            Err(CheckError::Invalid) => Err(GwError::new("unauthenticated", "токен не подходит")),
        }
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Fake {
        calls: AtomicUsize,
    }

    impl TokenCheck for Fake {
        fn validate(&self, token: &str) -> Result<(Principal, u64), CheckError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            match token {
                "good" => Ok((Principal { user_id: "u-1".to_string(), role: "user".to_string() }, 5_000)),
                "other" => Ok((Principal { user_id: "u-2".to_string(), role: "user".to_string() }, 6_000)),
                "old" => Err(CheckError::Expired),
                _ => Err(CheckError::Invalid),
            }
        }
    }

    fn fake() -> Fake {
        Fake { calls: AtomicUsize::new(0) }
    }

    fn header(value: &str) -> Vec<(String, String)> {
        vec![("Authorization".to_string(), value.to_string())]
    }

    #[test]
    fn valid_token_in_the_header_lives_in_the_context() {
        let checker = fake();
        let conn = Connection::open(&header("Bearer good"), "", &checker).expect("токен годный");
        assert_eq!(conn.principal().map(|p| p.user_id.as_str()), Some("u-1"), "Principal лежит в соединении");
        assert_eq!(conn.exp(), Some(5_000), "и срок токена тоже: он пригодится на каждом вызове");
        assert_eq!(checker.calls.load(Ordering::SeqCst), 1, "токен проверен ровно один раз");
    }

    #[test]
    fn browser_clients_pass_the_token_in_the_query() {
        let checker = fake();
        let conn = Connection::open(&[], "v=2&token=good&x=1", &checker).expect("токен в строке запроса");
        assert_eq!(conn.principal().map(|p| p.role.as_str()), Some("user"), "Principal из токена запроса");
    }

    #[test]
    fn header_beats_the_query() {
        let checker = fake();
        let conn = Connection::open(&header("bearer other"), "token=good", &checker).unwrap();
        assert_eq!(conn.principal().map(|p| p.user_id.as_str()), Some("u-2"), "заголовок главнее, схема без учёта регистра");
    }

    #[test]
    fn no_token_means_anonymous_and_costs_no_validation() {
        let checker = fake();
        let conn = Connection::open(&[], "v=2", &checker).expect("аноним допустим");
        assert!(conn.principal().is_none() && conn.exp().is_none(), "анонимное соединение");
        assert_eq!(checker.calls.load(Ordering::SeqCst), 0, "проверять нечего");
    }

    #[test]
    fn bad_tokens_refuse_the_connection_with_telling_codes() {
        let checker = fake();
        assert_eq!(Connection::open(&header("Bearer old"), "", &checker).unwrap_err().code, "token_expired", "просрочен: клиент обновит токен и придёт снова");
        assert_eq!(Connection::open(&header("Bearer junk"), "", &checker).unwrap_err().code, "unauthenticated", "мусор");
        assert_eq!(Connection::open(&[], "token=junk", &checker).unwrap_err().code, "unauthenticated", "мусор в запросе");
        assert_eq!(Connection::open(&header("Basic abc"), "", &checker).unwrap_err().code, "unauthenticated", "чужая схема");
        assert_eq!(Connection::open(&header("Bearer "), "", &checker).unwrap_err().code, "unauthenticated", "пустой токен");
    }

    #[test]
    fn ambiguity_is_refused_without_asking_the_checker() {
        let checker = fake();
        assert_eq!(Connection::open(&[], "token=good&token=other", &checker).unwrap_err().code, "bad_request", "два токена в запросе");
        assert_eq!(checker.calls.load(Ordering::SeqCst), 0, "неоднозначный запрос до проверки не доходит");
    }
}
