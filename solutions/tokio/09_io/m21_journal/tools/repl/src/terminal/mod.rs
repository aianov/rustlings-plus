use std::error::Error;
use std::fmt;
use std::path::Path;

use contracts::{ChatCommand, Command, ParseError, Username};
use social_core::{
    App, CommandError, CommandTable, IntoResponse, Notifiers, Response, social_table,
};
use tokio::runtime::Runtime;

use crate::runtime::{self, CLOSE_DEADLINE};
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

impl IntoResponse for SessionError {
    fn into_response(self) -> Response {
        let status = match self {
            SessionError::NotLoggedIn => 401,
            SessionError::AlreadyLoggedIn(..) | SessionError::NoPeer => 409,
        };
        Response::new(status, self.to_string())
    }
}

impl IntoResponse for TerminalError {
    fn into_response(self) -> Response {
        match self {
            TerminalError::Command(error) => error.into_response(),
            TerminalError::Session(error) => error.into_response(),
        }
    }
}

pub struct Terminal {
    runtime: Runtime,
    app: App,
    table: &'static CommandTable,
    session: Session,
}

impl Terminal {
    pub fn new() -> crate::Result<Terminal> {
        Terminal::with_notifiers(Notifiers::new())
    }

    pub fn with_notifiers(notifiers: Notifiers) -> crate::Result<Terminal> {
        Terminal::with_journal(notifiers, None)
    }

    pub fn with_journal(notifiers: Notifiers, journal: Option<&Path>) -> crate::Result<Terminal> {
        let runtime = runtime::start()?;
        let app = runtime.block_on(runtime::open(notifiers, journal))?;
        Ok(Terminal {
            runtime,
            app,
            table: social_table(),
            session: Session::Anonymous,
        })
    }

    pub fn session(&self) -> Option<&Session> {
        Some(&self.session)
    }

    pub fn handle(&mut self, line: &str) -> Response {
        let line = line.trim();
        if line.starts_with('/') {
            self.chat(line).into_response()
        } else {
            self.network(line).into_response()
        }
    }

    fn network(&self, line: &str) -> Result<String, CommandError> {
        let command: Command = line.parse()?;
        self.runtime.block_on(self.table.call(&self.app, command))
    }

    fn chat(&mut self, line: &str) -> Result<String, TerminalError> {
        let command: ChatCommand = line.parse()?;
        let (next, action) = self.session.step(command)?;
        let reply = match action {
            Action::Login(user) => {
                self.ensure_registered(&user)?;
                next.to_string()
            }
            Action::Logout(user) => format!("до встречи, {user}"),
            Action::Whoami => next.to_string(),
            Action::Run(command) => self.runtime.block_on(self.table.call(&self.app, command))?,
        };
        self.session = next;
        Ok(reply)
    }

    fn ensure_registered(&self, user: &Username) -> Result<(), CommandError> {
        let found = self.runtime.block_on(self.app.find_user(user.as_ref()));
        if found.is_some() {
            Ok(())
        } else {
            Err(CommandError::UnknownName(user.as_ref().to_string()))
        }
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        self.runtime.block_on(self.app.close(CLOSE_DEADLINE));
    }
}
