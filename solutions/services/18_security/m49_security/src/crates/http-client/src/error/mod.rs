use std::error::Error as StdError;
use std::fmt;
use std::io;

use crate::url::{RedirectError, UrlError};
use crate::wire::RequestError;

#[derive(Debug)]
pub enum Error {
    Url(UrlError),
    Request(RequestError),
    ConnectTimeout,
    TotalTimeout,
    Connect(io::Error),
    Tls(io::Error),
    Io(io::Error),
    Protocol(String),
    TooManyRedirects,
    BadRedirect(RedirectError),
    Status { code: u16, body: String },
    Decode(String),
    Forbidden(String),
}

impl Error {
    pub fn status(&self) -> Option<u16> {
        match self {
            Error::Status { code, .. } => Some(*code),
            _ => None,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Url(error) => write!(f, "плохой URL: {error}"),
            Error::Request(error) => write!(f, "запрос не собрался: {error}"),
            Error::ConnectTimeout => write!(f, "сервер не принял соединение вовремя"),
            Error::TotalTimeout => write!(f, "запрос не уложился в общий дедлайн"),
            Error::Connect(..) => write!(f, "не подключиться к серверу"),
            Error::Tls(..) => write!(f, "TLS с сервером не сложился"),
            Error::Io(..) => write!(f, "соединение оборвалось"),
            Error::Protocol(why) => write!(f, "сервер ответил не по HTTP: {why}"),
            Error::TooManyRedirects => write!(f, "слишком много редиректов"),
            Error::BadRedirect(error) => write!(f, "редирект не годится: {error}"),
            Error::Status { code, body } => write!(f, "сервер ответил {code}: {body}"),
            Error::Decode(why) => write!(f, "тело ответа не разобралось: {why}"),
            Error::Forbidden(host) => write!(
                f,
                "адрес {host} закрыт: внутренние сети не для запросов наружу"
            ),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Error::Connect(error) | Error::Tls(error) | Error::Io(error) => Some(error),
            _ => None,
        }
    }
}
