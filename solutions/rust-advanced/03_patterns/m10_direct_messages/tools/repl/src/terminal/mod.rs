use std::error::Error;
use std::fmt;
use std::io;

use contracts::{ChatCommand, Input, ParseError, Username};
use social_core::{CommandError, MemNetwork, MemPostStore, MemUserStore, SharedNetwork, execute};

use crate::session::{Action, Session, SessionError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalError {
    Command(CommandError),
    Session(SessionError),
}

impl From<ParseError> for TerminalError {
    fn from(error: ParseError) -> TerminalError {
        TerminalError::Command(CommandError::Parse(error))
    }
}

impl From<CommandError> for TerminalError {
    fn from(error: CommandError) -> TerminalError {
        TerminalError::Command(error)
    }
}

impl From<SessionError> for TerminalError {
    fn from(error: SessionError) -> TerminalError {
        TerminalError::Session(error)
    }
}

impl fmt::Display for TerminalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TerminalError::Command(error) => write!(f, "{error}"),
            TerminalError::Session(error) => write!(f, "{error}"),
        }
    }
}

impl Error for TerminalError {}

pub struct Terminal {
    network: SharedNetwork<MemUserStore, MemPostStore>,
    session: Session,
}

impl Terminal {
    pub fn new() -> io::Result<Terminal> {
        Ok(Terminal {
            network: SharedNetwork::new(MemNetwork::new())?,
            session: Session::Anonymous,
        })
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    pub fn handle(&mut self, line: &str) -> Result<String, TerminalError> {
        match line.parse::<Input>()? {
            Input::Command(command) => Ok(execute(&command, &self.network)?),
            Input::Chat(command) => self.chat(command),
        }
    }

    fn chat(&mut self, command: ChatCommand) -> Result<String, TerminalError> {
        let (next, action) = self.session.step(command)?;
        let reply = match action {
            Action::Login(user) => {
                self.ensure_registered(&user)?;
                next.to_string()
            }
            Action::Logout(user) => format!("до встречи, {user}"),
            Action::Whoami => next.to_string(),
            Action::Run(command) => execute(&command, &self.network)?,
        };
        self.session = next;
        Ok(reply)
    }

    fn ensure_registered(&self, user: &Username) -> Result<(), CommandError> {
        if self
            .network
            .read(|network| network.is_registered(user.as_ref()))
        {
            Ok(())
        } else {
            Err(CommandError::UnknownName(user.as_ref().to_string()))
        }
    }
}
