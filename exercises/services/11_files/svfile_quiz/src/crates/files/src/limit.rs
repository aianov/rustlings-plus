use crate::Body;

#[derive(Debug, PartialEq)]
pub struct TooLarge {
    pub limit: u64,
}

pub fn limit(body: Body, max: u64, declared: Option<u64>) -> Result<Body, TooLarge> {
    let _ = (body, max, declared);
    todo!()
}
