#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Say(String),
    Login(String),
    Logout,
    Whoami,
    Msg { to: String, text: String },
    History { with: String, limit: Option<usize> },
    Quit,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParseError {
    Empty,
    Unknown(String),
    MissingArg { command: &'static str, what: &'static str },
    TooManyArgs(&'static str),
    InvalidNick(String),
    BadNumber(String),
    LimitOutOfRange(usize),
}

pub fn parse(line: &str) -> Result<Command, ParseError> {
    let line = line.trim();
    if line.is_empty() {
        return Err(ParseError::Empty);
    }
    if let Some(escaped) = line.strip_prefix("//") {
        return Ok(Command::Say(format!("/{escaped}")));
    }
    let Some(body) = line.strip_prefix('/') else {
        return Ok(Command::Say(line.to_string()));
    };
    if body.starts_with(char::is_whitespace) {
        return Err(ParseError::Unknown(String::new()));
    }
    let words: Vec<&str> = body.split_whitespace().collect();
    match words.as_slice() {
        [] => Err(ParseError::Unknown(String::new())),
        ["login"] => missing("login", "nick"),
        ["login", nick] => Ok(Command::Login(valid_nick(nick)?)),
        ["login", ..] => Err(ParseError::TooManyArgs("login")),
        ["logout"] => Ok(Command::Logout),
        ["logout", ..] => Err(ParseError::TooManyArgs("logout")),
        ["whoami"] => Ok(Command::Whoami),
        ["whoami", ..] => Err(ParseError::TooManyArgs("whoami")),
        ["quit"] => Ok(Command::Quit),
        ["quit", ..] => Err(ParseError::TooManyArgs("quit")),
        ["msg"] => missing("msg", "nick"),
        ["msg", _] => missing("msg", "text"),
        ["msg", nick, text @ ..] => Ok(Command::Msg { to: valid_nick(nick)?, text: text.join(" ") }),
        ["history"] => missing("history", "nick"),
        ["history", nick] => Ok(Command::History { with: valid_nick(nick)?, limit: None }),
        ["history", nick, limit] => {
            let with = valid_nick(nick)?;
            let Ok(limit) = limit.parse::<usize>() else {
                return Err(ParseError::BadNumber(limit.to_string()));
            };
            let limit @ 1..=100 = limit else {
                return Err(ParseError::LimitOutOfRange(limit));
            };
            Ok(Command::History { with, limit: Some(limit) })
        }
        ["history", ..] => Err(ParseError::TooManyArgs("history")),
        [other, ..] => Err(ParseError::Unknown(other.to_string())),
    }
}

fn missing(command: &'static str, what: &'static str) -> Result<Command, ParseError> {
    Err(ParseError::MissingArg { command, what })
}

fn valid_nick(nick: &str) -> Result<String, ParseError> {
    let shape_ok = (3..=16).contains(&nick.len()) && nick.chars().all(|c| matches!(c, 'a'..='z' | '0'..='9' | '_'));
    if shape_ok { Ok(nick.to_string()) } else { Err(ParseError::InvalidNick(nick.to_string())) }
}
