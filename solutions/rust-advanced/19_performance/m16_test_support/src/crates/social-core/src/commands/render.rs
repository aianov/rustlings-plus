use std::fmt::{self, Write};

use crate::domain::{Post, User, UserId};
use crate::network::Network;
use crate::ports::{PostStore, UserStore};

const LINE_BYTES: usize = 128;

const MAX_RESERVED_LINES: usize = 100;

pub struct Name<'a> {
    user: Option<&'a User>,
    id: UserId,
}

impl<'a> Name<'a> {
    pub fn new(user: Option<&'a User>, id: UserId) -> Name<'a> {
        Name { user, id }
    }
}

impl fmt::Display for Name<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.user {
            Some(user) => write!(f, "@{}", user.name()),
            None => write!(f, "id {}", self.id.0),
        }
    }
}

pub struct Text {
    out: String,
}

impl Text {
    pub fn with_lines(lines: usize) -> Text {
        Text {
            out: String::with_capacity(lines.min(MAX_RESERVED_LINES) * LINE_BYTES),
        }
    }

    pub fn line(&mut self, line: impl fmt::Display) {
        if !self.out.is_empty() {
            self.out.push('\n');
        }
        let _ = write!(self.out, "{line}");
    }

    pub fn post<U: UserStore, P: PostStore>(&mut self, network: &Network<U, P>, post: &Post) {
        let author = Name::new(network.user(post.author()), post.author());
        self.line(format_args!("{author} {post}"));
    }

    pub fn or(self, empty: &str) -> String {
        if self.out.is_empty() {
            empty.to_string()
        } else {
            self.out
        }
    }
}
