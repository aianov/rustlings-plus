use serde::Deserialize;
use std::borrow::Cow;

#[derive(Debug, Deserialize)]
pub struct Request<'a> {
    pub user: u64,
    pub limit: usize,
    #[serde(borrow, default)]
    pub tag: Cow<'a, str>,
}

pub fn parse_request(body: &str) -> Result<Request<'_>, String> {
    serde_json::from_str(body).map_err(|e| e.to_string())
}
