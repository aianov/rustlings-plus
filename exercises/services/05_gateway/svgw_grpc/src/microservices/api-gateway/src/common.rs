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

#[derive(Debug, Clone, PartialEq)]
pub struct RequestCtx {
    pub request_id: String,
    pub user_id: Option<String>,
}
