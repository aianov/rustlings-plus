use std::str::FromStr;

use crate::chat::ChatCommand;
use crate::command::{Command, ParseError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    Command(Command),
    Chat(ChatCommand),
}

impl FromStr for Input {
    type Err = ParseError;

    fn from_str(line: &str) -> Result<Input, ParseError> {
        if line.trim_start().starts_with('/') {
            line.parse().map(Input::Chat)
        } else {
            line.parse().map(Input::Command)
        }
    }
}
