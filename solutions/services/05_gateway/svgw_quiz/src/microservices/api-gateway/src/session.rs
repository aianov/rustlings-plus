use crate::types::{CheckError, GwError, Principal, TokenCheck};

#[derive(Debug)]
pub struct Session {
    pub principal: Option<Principal>,
    pub exp: Option<u64>,
    pub closed: bool,
}

impl Session {
    pub fn open(headers: &[(String, String)], query: &str, checker: &dyn TokenCheck) -> Result<Session, GwError> {
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
            return Ok(Session { principal: None, exp: None, closed: false });
        };
        match checker.validate(token) {
            Ok((principal, exp)) => Ok(Session { principal: Some(principal), exp: Some(exp), closed: false }),
            Err(CheckError::Expired) => Err(GwError::new("token_expired", "токен просрочен")),
            Err(CheckError::Invalid) => Err(GwError::new("unauthenticated", "токен не подходит")),
        }
    }

    pub fn ensure_alive(&mut self, now: u64) -> Result<(), GwError> {
        if self.closed {
            return Err(GwError::new("closed", "соединение закрыто, подключись заново"));
        }
        if self.exp.is_some_and(|exp| now >= exp) {
            self.closed = true;
            return Err(GwError::new("token_expired", "токен просрочен, подключись заново"));
        }
        Ok(())
    }
}
