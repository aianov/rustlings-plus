use std::str::FromStr;

use crate::command::ParseError;
use crate::username::Username;

pub const DEFAULT_HISTORY_LIMIT: usize = 20;
pub const MAX_HISTORY_LIMIT: usize = 100;

pub const CHAT_COMMAND_NAMES: [&str; 6] =
    ["/login", "/logout", "/whoami", "/msg", "/say", "/history"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatCommand {
    Login(Username),
    Logout,
    Whoami,
    Msg { to: Username, text: String },
    Say(String),
    History { with: Username, limit: usize },
}

impl FromStr for ChatCommand {
    type Err = ParseError;

    fn from_str(line: &str) -> Result<ChatCommand, ParseError> {
        let words: Vec<&str> = line.split_whitespace().collect();
        let [first, args @ ..] = words.as_slice() else {
            return Err(ParseError::Empty);
        };
        let Some(name) = first.strip_prefix('/') else {
            return Err(ParseError::UnknownCommand(first.to_string()));
        };
        match (name, args) {
            ("login", []) => Err(ParseError::MissingArgument("ник")),
            ("login", [nick]) => Ok(ChatCommand::Login(nick_of(nick)?)),
            ("logout", []) => Ok(ChatCommand::Logout),
            ("whoami", []) => Ok(ChatCommand::Whoami),
            ("msg", []) => Err(ParseError::MissingArgument("кому")),
            ("msg", [_]) | ("say", []) => Err(ParseError::MissingArgument("текст сообщения")),
            ("msg", [to, text @ ..]) => Ok(ChatCommand::Msg {
                to: nick_of(to)?,
                text: text.join(" "),
            }),
            ("say", text) => Ok(ChatCommand::Say(text.join(" "))),
            ("history", []) => Err(ParseError::MissingArgument("с кем")),
            ("history", [with]) => Ok(ChatCommand::History {
                with: nick_of(with)?,
                limit: DEFAULT_HISTORY_LIMIT,
            }),
            ("history", [with, limit]) => Ok(ChatCommand::History {
                with: nick_of(with)?,
                limit: history_limit(limit)?,
            }),
            ("login", [_, extra, ..])
            | ("logout" | "whoami", [extra, ..])
            | ("history", [_, _, extra, ..]) => Err(ParseError::ExtraArgument(extra.to_string())),
            (other, _) => Err(ParseError::UnknownCommand(format!("/{other}"))),
        }
    }
}

fn nick_of(word: &str) -> Result<Username, ParseError> {
    Username::try_from(word).map_err(ParseError::BadName)
}

fn history_limit(word: &str) -> Result<usize, ParseError> {
    let limit: usize = word
        .parse()
        .map_err(|_| ParseError::BadNumber(word.to_string()))?;
    let limit @ 1..=MAX_HISTORY_LIMIT = limit else {
        return Err(ParseError::LimitOutOfRange(limit));
    };
    Ok(limit)
}
