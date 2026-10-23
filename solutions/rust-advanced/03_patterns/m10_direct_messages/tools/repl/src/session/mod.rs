use std::error::Error;
use std::fmt;

use contracts::{ChatCommand, Command, Username};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Session {
    Anonymous,
    LoggedIn { user: Username },
    Chatting { user: Username, peer: Username },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Login(Username),
    Logout(Username),
    Whoami,
    Run(Command),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionError {
    NotLoggedIn,
    AlreadyLoggedIn(Username),
    NoPeer,
}

impl Session {
    pub fn step(&self, command: ChatCommand) -> Result<(Session, Action), SessionError> {
        match (self, command) {
            (Session::Anonymous, ChatCommand::Login(user)) => Ok((
                Session::LoggedIn { user: user.clone() },
                Action::Login(user),
            )),
            (Session::Anonymous, ChatCommand::Whoami) => Ok((Session::Anonymous, Action::Whoami)),
            (
                Session::Anonymous,
                ChatCommand::Logout
                | ChatCommand::Msg { .. }
                | ChatCommand::Say(..)
                | ChatCommand::History { .. },
            ) => Err(SessionError::NotLoggedIn),
            (
                Session::LoggedIn { user } | Session::Chatting { user, .. },
                ChatCommand::Login(..),
            ) => Err(SessionError::AlreadyLoggedIn(user.clone())),
            (Session::LoggedIn { user } | Session::Chatting { user, .. }, ChatCommand::Logout) => {
                Ok((Session::Anonymous, Action::Logout(user.clone())))
            }
            (Session::LoggedIn { .. } | Session::Chatting { .. }, ChatCommand::Whoami) => {
                Ok((self.clone(), Action::Whoami))
            }
            (
                Session::LoggedIn { user } | Session::Chatting { user, .. },
                ChatCommand::Msg { to, text },
            ) => Ok((
                Session::Chatting {
                    user: user.clone(),
                    peer: to.clone(),
                },
                send(user, to, text),
            )),
            (Session::LoggedIn { .. }, ChatCommand::Say(..)) => Err(SessionError::NoPeer),
            (Session::Chatting { user, peer }, ChatCommand::Say(text)) => {
                Ok((self.clone(), send(user, peer.clone(), text)))
            }
            (
                Session::LoggedIn { user } | Session::Chatting { user, .. },
                ChatCommand::History { with, limit },
            ) => Ok((
                self.clone(),
                Action::Run(Command::History {
                    who: user.clone(),
                    with,
                    limit,
                }),
            )),
        }
    }
}

fn send(from: &Username, to: Username, text: String) -> Action {
    Action::Run(Command::SendMessage {
        from: from.clone(),
        to,
        text,
    })
}

impl fmt::Display for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Session::Anonymous => write!(f, "вход не выполнен"),
            Session::LoggedIn { user } => write!(f, "ты {user}"),
            Session::Chatting { user, peer } => write!(f, "ты {user}, диалог с {peer}"),
        }
    }
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SessionError::NotLoggedIn => write!(f, "сначала войди: /login <ник>"),
            SessionError::AlreadyLoggedIn(user) => {
                write!(f, "сессия {user} уже открыта, сначала /logout")
            }
            SessionError::NoPeer => write!(f, "диалог не открыт, начни его: /msg <ник> <текст>"),
        }
    }
}

impl Error for SessionError {}
