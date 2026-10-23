use std::error::Error;
use std::fmt;
use std::path::Path;
use std::pin::Pin;

use contracts::{ChatCommand, Command, ParseError, Username};
use futures::{FutureExt, Stream, StreamExt};
use social_core::{
    App, AppError, CommandError, CommandTable, FeedEvent, IntoResponse, Notifiers, Op, PostItem,
    Response, UserId, social_table,
};
use tokio::runtime::Runtime;
use tokio::time::timeout;

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

type Feed = Pin<Box<dyn Stream<Item = Result<FeedEvent, AppError>> + Send>>;

struct Watching {
    user: Username,
    feed: Feed,
}

pub struct Terminal {
    runtime: Runtime,
    app: App,
    table: &'static CommandTable,
    session: Session,
    watching: Option<Watching>,
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
            watching: None,
        })
    }

    pub fn session(&self) -> Option<&Session> {
        Some(&self.session)
    }

    pub fn watching(&self) -> Option<&Username> {
        self.watching.as_ref().map(|watching| &watching.user)
    }

    pub fn live(&mut self) -> Vec<String> {
        let Some(watching) = self.watching.as_mut() else {
            return Vec::new();
        };
        let _entered = self.runtime.enter();
        let mut lines = Vec::new();
        let mut ended = false;
        loop {
            match watching.feed.next().now_or_never() {
                Some(Some(Ok(FeedEvent::Live(post) | FeedEvent::History(post)))) => {
                    lines.push(format!("вживую: {}", line_of(&post)));
                }
                Some(Some(Ok(FeedEvent::UpToDate))) => {}
                Some(Some(Err(error))) => lines.push(format!("ошибка ленты: {error}")),
                Some(None) => {
                    ended = true;
                    break;
                }
                None => break,
            }
        }
        if ended {
            self.watching = None;
        }
        lines
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
            Action::Logout(user) => {
                self.watching = None;
                format!("до встречи, {user}")
            }
            Action::Whoami => next.to_string(),
            Action::Run(command) => self.runtime.block_on(self.table.call(&self.app, command))?,
            Action::Watch { user, recent } => self.watch(user, recent)?,
            Action::Unwatch => self.unwatch(),
        };
        self.session = next;
        Ok(reply)
    }

    fn ensure_registered(&self, user: &Username) -> Result<(), CommandError> {
        self.id_of(user).map(drop)
    }

    fn id_of(&self, user: &Username) -> Result<UserId, CommandError> {
        let found = self.runtime.block_on(self.app.find_user(user.as_ref()));
        found
            .map(|found| found.id())
            .ok_or_else(|| CommandError::UnknownName(user.as_ref().to_string()))
    }

    fn watch(&mut self, user: Username, recent: usize) -> Result<String, CommandError> {
        let viewer = self.id_of(&user)?;
        let mut feed: Feed = Box::pin(self.app.watch(viewer, recent));
        let after = self.app.config().op_timeout;
        let history = self.runtime.block_on(async {
            timeout(after, async {
                let mut lines = Vec::new();
                while let Some(event) = feed.next().await {
                    match event? {
                        FeedEvent::History(post) => lines.push(line_of(&post)),
                        FeedEvent::Live(post) => lines.push(format!("вживую: {}", line_of(&post))),
                        FeedEvent::UpToDate => break,
                    }
                }
                Ok::<_, AppError>(lines)
            })
            .await
        });
        let op = Op::Job("лента");
        let mut lines = history.map_err(|_| AppError::Timeout { op, after })??;
        if lines.is_empty() {
            lines.push("лента пуста".to_string());
        }
        lines.push(format!(
            "слежу за лентой {user}, новые посты появятся здесь"
        ));
        self.watching = Some(Watching { user, feed });
        Ok(lines.join("\n"))
    }

    fn unwatch(&mut self) -> String {
        match self.watching.take() {
            Some(watching) => format!("больше не слежу за лентой {}", watching.user),
            None => "за лентой никто не следит".to_string(),
        }
    }
}

fn line_of(post: &PostItem) -> String {
    format!("@{} {post}", post.author_name)
}

impl Drop for Terminal {
    fn drop(&mut self) {
        self.runtime.block_on(self.app.close(CLOSE_DEADLINE));
    }
}
