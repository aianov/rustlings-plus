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
    todo!("разобрать строку чата в команду, см. README")
}
