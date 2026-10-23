pub struct Req<'a> {
    pub method: &'a str,
    pub target: &'a str,
    pub headers: &'a [(String, String)],
}

pub fn access_line(req: &Req<'_>, status: u16) -> String {
    let _ = (req, status);
    todo!()
}
