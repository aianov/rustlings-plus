use crate::command::Command;

#[derive(Debug, Clone, PartialEq)]
pub enum Session {
    Anonymous,
    LoggedIn { user: String },
    Chatting { user: String, peer: String },
    Closed,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Reply {
    Welcome(String),
    Goodbye,
    You(Option<String>),
    Sent { to: String, text: String },
    Show { with: String, limit: Option<usize> },
    Bye,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Reason {
    NotLoggedIn,
    AlreadyLoggedIn,
    NoPeer,
    SelfMessage,
    SessionClosed,
}

#[derive(Debug, PartialEq)]
pub struct Rejected {
    pub session: Session,
    pub reason: Reason,
}

fn reject(session: Session, reason: Reason) -> Result<(Session, Reply), Rejected> {
    Err(Rejected { session, reason })
}

impl Session {
    pub fn step(self, command: Command) -> Result<(Session, Reply), Rejected> {
        match self {
            Session::Anonymous => match command {
                Command::Login(user) => Ok((Session::LoggedIn { user: user.clone() }, Reply::Welcome(user))),
                Command::Whoami => Ok((Session::Anonymous, Reply::You(None))),
                Command::Quit => Ok((Session::Closed, Reply::Bye)),
                Command::Logout | Command::Say(_) | Command::Msg { .. } | Command::History { .. } => {
                    reject(Session::Anonymous, Reason::NotLoggedIn)
                }
            },
            Session::LoggedIn { user } => match command {
                Command::Login(_) => reject(Session::LoggedIn { user }, Reason::AlreadyLoggedIn),
                Command::Logout => Ok((Session::Anonymous, Reply::Goodbye)),
                Command::Whoami => {
                    let reply = Reply::You(Some(user.clone()));
                    Ok((Session::LoggedIn { user }, reply))
                }
                Command::Msg { to, .. } if to == user => reject(Session::LoggedIn { user }, Reason::SelfMessage),
                Command::Msg { to, text } => {
                    Ok((Session::Chatting { user, peer: to.clone() }, Reply::Sent { to, text }))
                }
                Command::History { with, limit } => Ok((Session::LoggedIn { user }, Reply::Show { with, limit })),
                Command::Say(_) => reject(Session::LoggedIn { user }, Reason::NoPeer),
                Command::Quit => Ok((Session::Closed, Reply::Bye)),
            },
            Session::Chatting { user, peer } => match command {
                Command::Login(_) => reject(Session::Chatting { user, peer }, Reason::AlreadyLoggedIn),
                Command::Logout => Ok((Session::Anonymous, Reply::Goodbye)),
                Command::Whoami => {
                    let reply = Reply::You(Some(user.clone()));
                    Ok((Session::Chatting { user, peer }, reply))
                }
                Command::Say(text) => Ok((Session::Chatting { user, peer: peer.clone() }, Reply::Sent { to: peer, text })),
                Command::Msg { to, .. } if to == user => reject(Session::Chatting { user, peer }, Reason::SelfMessage),
                Command::Msg { to, text } => {
                    Ok((Session::Chatting { user, peer: to.clone() }, Reply::Sent { to, text }))
                }
                Command::History { with, limit } => Ok((Session::Chatting { user, peer }, Reply::Show { with, limit })),
                Command::Quit => Ok((Session::Closed, Reply::Bye)),
            },
            Session::Closed => reject(Session::Closed, Reason::SessionClosed),
        }
    }
}
