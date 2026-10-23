use crate::image::sniff;

pub struct Meta {
    pub client_name: String,
}

#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    pub fn empty(status: u16) -> Response {
        Response { status, headers: vec![("X-Content-Type-Options".into(), "nosniff".into())], body: Vec::new() }
    }
}

pub fn serve(meta: &Meta, content: &[u8]) -> Response {
    let _ = (meta, sniff(content));
    todo!()
}
