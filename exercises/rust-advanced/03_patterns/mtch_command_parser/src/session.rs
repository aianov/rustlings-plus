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

impl Session {
    pub fn step(self, command: Command) -> Result<(Session, Reply), Rejected> {
        todo!("таблица переходов из README")
    }
}
