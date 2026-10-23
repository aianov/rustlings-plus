use crate::{Method, Middleware, Request, Response, handler, middleware};

#[derive(Debug, Clone)]
pub struct CorsPolicy {
    pub origins: Vec<String>,
    pub methods: Vec<String>,
    pub headers: Vec<String>,
    pub expose_headers: Vec<String>,
    pub credentials: bool,
    pub max_age: u32,
}

impl CorsPolicy {
    fn allows(&self, origin: &str) -> bool {
        self.origins.iter().any(|o| o == "*" || o == origin)
    }

    fn constant_star(&self) -> bool {
        self.origins.iter().any(|o| o == "*") && !self.credentials
    }
}

fn preflight(policy: &CorsPolicy, request: &Request, origin: &str, method: &str) -> Response {
    let vary = "Origin, Access-Control-Request-Method, Access-Control-Request-Headers";
    let requested = request.headers.get("access-control-request-headers").unwrap_or("");
    let headers_ok = requested.split(',').map(str::trim).filter(|h| !h.is_empty()).all(|h| policy.headers.iter().any(|a| a.eq_ignore_ascii_case(h)));
    let mut response = Response::new(403);
    response.headers.append("Vary", vary);
    if !(policy.allows(origin) && policy.methods.iter().any(|m| m == method) && headers_ok) {
        return response;
    }
    let mut response = Response::new(204);
    response.headers.append("Access-Control-Allow-Origin", if policy.constant_star() { "*" } else { origin });
    response.headers.append("Access-Control-Allow-Methods", &policy.methods.join(", "));
    response.headers.append("Access-Control-Allow-Headers", &policy.headers.join(", "));
    response.headers.append("Access-Control-Max-Age", &policy.max_age.to_string());
    if policy.credentials {
        response.headers.append("Access-Control-Allow-Credentials", "true");
    }
    response.headers.append("Vary", vary);
    response
}

pub fn cors(policy: CorsPolicy) -> Middleware {
    middleware(move |next| {
        let policy = policy.clone();
        handler(move |request: Request| {
            let next = next.clone();
            let policy = policy.clone();
            async move {
                let _ = (&policy, &next, request);
                todo!()
            }
        })
    })
}
