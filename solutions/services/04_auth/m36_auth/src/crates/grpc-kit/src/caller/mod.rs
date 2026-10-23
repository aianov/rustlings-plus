use proto::domain;
use tonic::{Request, Status};

pub const USER_ID: &str = "x-user-id";

pub const USER_ROLE: &str = "x-user-role";

pub const GUEST: &str = "guest";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caller(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallerRole(pub String);

pub fn caller(mut request: Request<()>) -> Result<Request<()>, Status> {
    let role = request
        .metadata()
        .get(USER_ROLE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    if let Some(role) = role {
        request.extensions_mut().insert(CallerRole(role));
    }
    let Some(value) = request.metadata().get(USER_ID) else {
        return Ok(request);
    };
    let id = value
        .to_str()
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .ok_or_else(|| Status::invalid_argument("x-user-id: ждали число"))?;
    request.extensions_mut().insert(Caller(id));
    Ok(request)
}

pub fn caller_of<T>(request: &Request<T>) -> Result<u64, Status> {
    request
        .extensions()
        .get::<Caller>()
        .map(|caller| caller.0)
        .ok_or_else(|| domain("unauthorized", "вызов без x-user-id: кто действует?"))
}

pub fn role_of<T>(request: &Request<T>) -> &str {
    request
        .extensions()
        .get::<CallerRole>()
        .map_or(GUEST, |role| role.0.as_str())
}
