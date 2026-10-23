use std::error::Error;
use std::fmt;

use contracts::{CHAT_COMMAND_NAMES, ChatCommand, ParseError, Username};
use social_core::{
    CommandBus, CommandError, Context, IntoResponse, MemNetwork, MemPostStore, MemUserStore,
    Notifiers, Response, SharedNetwork, execute, network_commands,
};

use crate::session::{Action, Session, SessionError};

type Shared = SharedNetwork<MemUserStore, MemPostStore>;

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
    bus: CommandBus,
    context: Context,
}

impl Terminal {
    pub fn new() -> crate::Result<Terminal> {
        Terminal::with_notifiers(Notifiers::new())
    }

    pub fn with_notifiers(notifiers: Notifiers) -> crate::Result<Terminal> {
        let mut bus = CommandBus::new();
        network_commands::<MemUserStore, MemPostStore>(&mut bus);
        for name in CHAT_COMMAND_NAMES {
            bus.register(name, chat);
        }
        let mut context = Context::new();
        context.insert(Shared::with_notifiers(MemNetwork::new(), notifiers)?);
        context.insert(Session::Anonymous);
        Ok(Terminal { bus, context })
    }

    pub fn session(&self) -> Option<&Session> {
        self.context.get::<Session>()
    }

    pub fn handle(&mut self, line: &str) -> Response {
        self.bus.dispatch(&mut self.context, line)
    }
}

fn chat(ctx: &mut Context, line: &str) -> Result<String, TerminalError> {
    let command: ChatCommand = line.parse()?;
    let (next, action) = ctx.require::<Session>()?.step(command)?;
    let network = ctx.require::<Shared>()?;
    let reply = match action {
        Action::Login(user) => {
            ensure_registered(network, &user)?;
            next.to_string()
        }
        Action::Logout(user) => format!("до встречи, {user}"),
        Action::Whoami => next.to_string(),
        Action::Run(command) => execute(&command, network)?,
    };
    ctx.insert(next);
    Ok(reply)
}

fn ensure_registered(network: &Shared, user: &Username) -> Result<(), CommandError> {
    if network.read(|network| network.is_registered(user.as_ref())) {
        Ok(())
    } else {
        Err(CommandError::UnknownName(user.as_ref().to_string()))
    }
}
