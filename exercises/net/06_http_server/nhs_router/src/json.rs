use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::{Request, Response};

impl Response {
    pub fn json<T: Serialize>(status: u16, value: &T) -> Response {
        todo!()
    }

    pub fn error(status: u16, message: &str) -> Response {
        todo!()
    }
}

impl Request {
    pub fn json<T: DeserializeOwned>(&self) -> Result<T, Response> {
        todo!()
    }
}
