use std::error::Error;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BusError {
    Unavailable,
    NoService,
    Timeout,
    Encode(String),
    Decode(String),
    Remote(String),
    Transport(String),
}

impl fmt::Display for BusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BusError::Unavailable => write!(f, "нет связи с брокером"),
            BusError::NoService => write!(f, "на тему никто не подписан"),
            BusError::Timeout => write!(f, "ответ не пришёл вовремя"),
            BusError::Encode(error) => write!(f, "не удалось собрать JSON: {error}"),
            BusError::Decode(error) => write!(f, "не удалось разобрать сообщение: {error}"),
            BusError::Remote(error) => write!(f, "сервис ответил ошибкой: {error}"),
            BusError::Transport(error) => write!(f, "ошибка брокера: {error}"),
        }
    }
}

impl Error for BusError {}

pub(crate) fn transport(error: impl fmt::Display) -> BusError {
    BusError::Transport(error.to_string())
}
