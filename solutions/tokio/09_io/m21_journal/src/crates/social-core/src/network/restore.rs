use std::fmt::Display;

use contracts::Entry;

use crate::domain::DomainError;
use crate::journal::JournalError;
use crate::ports::{PostStore, UserStore};

use super::Network;

enum Fault {
    Rejected(DomainError),
    Diverged { expected: String, got: String },
}

impl From<DomainError> for Fault {
    fn from(error: DomainError) -> Fault {
        Fault::Rejected(error)
    }
}

impl Fault {
    fn at(self, line: usize) -> JournalError {
        match self {
            Fault::Rejected(error) => JournalError::Rejected { line, error },
            Fault::Diverged { expected, got } => JournalError::Diverged {
                line,
                expected,
                got,
            },
        }
    }
}

fn same<T: PartialEq + Display>(expected: T, got: T) -> Result<(), Fault> {
    if expected == got {
        Ok(())
    } else {
        Err(Fault::Diverged {
            expected: expected.to_string(),
            got: got.to_string(),
        })
    }
}

impl<U: UserStore, P: PostStore> Network<U, P> {
    pub fn restore(&mut self, entries: &[Entry]) -> Result<usize, JournalError> {
        for (index, entry) in entries.iter().enumerate() {
            self.apply(entry).map_err(|fault| fault.at(index + 1))?;
        }
        Ok(entries.len())
    }

    fn apply(&mut self, entry: &Entry) -> Result<(), Fault> {
        match entry {
            Entry::Registered { user, name } => same(*user, self.register(name)?),
            Entry::Followed { who, whom } => {
                self.follow(*who, *whom).map(drop).map_err(Fault::from)
            }
            Entry::Muted { who, whom } => self.mute(*who, *whom).map(drop).map_err(Fault::from),
            Entry::Posted {
                post,
                author,
                text,
                visibility,
                at,
            } => same(*post, self.publish_at(*author, text, *visibility, *at)?),
            Entry::Liked { post, by, at } => {
                self.like_at(*by, *post, *at).map(drop).map_err(Fault::from)
            }
            Entry::Deleted { post, by } => {
                self.delete_post(*by, *post).map(drop).map_err(Fault::from)
            }
            Entry::Commented {
                comment,
                post,
                author,
                text,
            } => same(*comment, self.comment(*author, *post, text)?),
            Entry::Replied {
                comment,
                parent,
                author,
                text,
            } => same(*comment, self.reply(*author, *parent, text)?),
            Entry::MessageSent { from, to, text } => self
                .send_message(*from, *to, text)
                .map(drop)
                .map_err(Fault::from),
        }
    }
}
