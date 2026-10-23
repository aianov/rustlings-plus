use contracts::ParseError;

use super::CommandError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub body: String,
}

impl Response {
    pub fn new(status: u16, body: impl Into<String>) -> Response {
        Response {
            status,
            body: body.into(),
        }
    }

    pub fn is_ok(&self) -> bool {
        self.status < 400
    }
}

pub trait IntoResponse {
    fn into_response(self) -> Response;
}

impl IntoResponse for Response {
    fn into_response(self) -> Response {
        self
    }
}

impl IntoResponse for String {
    fn into_response(self) -> Response {
        Response::new(200, self)
    }
}

impl IntoResponse for &'static str {
    fn into_response(self) -> Response {
        Response::new(200, self)
    }
}

impl IntoResponse for (u16, String) {
    fn into_response(self) -> Response {
        Response::new(self.0, self.1)
    }
}

impl<T: IntoResponse, E: IntoResponse> IntoResponse for Result<T, E> {
    fn into_response(self) -> Response {
        match self {
            Ok(value) => value.into_response(),
            Err(error) => {
                let response = error.into_response();
                if response.is_ok() {
                    Response::new(500, response.body)
                } else {
                    response
                }
            }
        }
    }
}

impl IntoResponse for CommandError {
    fn into_response(self) -> Response {
        let status = match &self {
            CommandError::Parse(ParseError::UnknownCommand(..)) | CommandError::UnknownName(..) => {
                404
            }
            CommandError::Parse(..) => 400,
            CommandError::Domain(..) => 422,
            CommandError::NoExtension(..) => 500,
        };
        Response::new(status, self.to_string())
    }
}
