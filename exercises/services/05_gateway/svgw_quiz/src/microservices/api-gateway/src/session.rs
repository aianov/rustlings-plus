use crate::types::{CheckError, GwError, Principal, TokenCheck};

#[derive(Debug)]
pub struct Session {
    pub principal: Option<Principal>,
    pub exp: Option<u64>,
    pub closed: bool,
}

impl Session {
    pub fn open(headers: &[(String, String)], query: &str, checker: &dyn TokenCheck) -> Result<Session, GwError> {
        todo!()
    }

    pub fn ensure_alive(&mut self, now: u64) -> Result<(), GwError> {
        todo!()
    }
}
