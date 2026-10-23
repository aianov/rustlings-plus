use std::fmt::{self, Write};

use crate::app::Card;
use crate::domain::{User, UserId};
use crate::ports::PostItem;

const LINE_BYTES: usize = 128;

const MAX_RESERVED_LINES: usize = 100;

pub struct Name<'a> {
    name: Option<&'a str>,
    id: UserId,
}

impl<'a> Name<'a> {
    pub fn of(user: Option<&'a User>, id: UserId) -> Name<'a> {
        Name {
            name: user.map(User::name),
            id,
        }
    }

    pub fn author(post: &'a PostItem) -> Name<'a> {
        let name = Some(post.author_name.as_ref()).filter(|name| !name.is_empty());
        Name {
            name,
            id: post.author,
        }
    }
}

impl fmt::Display for Name<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.name {
            Some(name) => write!(f, "@{name}"),
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

    pub fn post(&mut self, post: &PostItem) {
        self.line(format_args!("{} {post}", Name::author(post)));
    }

    pub fn trend(&mut self, post: &PostItem, likes: usize) {
        self.line(format_args!(
            "{} {post}, за сутки +{likes}",
            Name::author(post)
        ));
    }

    pub fn or(self, empty: &str) -> String {
        if self.out.is_empty() {
            empty.to_string()
        } else {
            self.out
        }
    }
}

pub fn card(text: &mut Text, viewer: UserId, card: &Card, posts: &[PostItem]) {
    if card.bio.is_empty() {
        text.line(format_args!("@{}", card.name));
    } else {
        text.line(format_args!("@{}: {}", card.name, card.bio));
    }
    text.line(format_args!(
        "подписчиков {}, подписок {}, постов {}, лайков {}",
        card.followers, card.following, card.posts, card.likes
    ));
    text.line(if viewer == card.id {
        "это ты"
    } else if card.followed {
        "ты подписан"
    } else {
        "ты не подписан"
    });
    for post in posts {
        text.line(post);
    }
}
