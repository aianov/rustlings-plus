mod error;
mod method;

use std::str::FromStr;

use crate::cursor::Cursor;
use crate::ids::{CommentId, PostId};
use crate::setting::Setting;
use crate::username::Username;

pub use error::ParseError;
pub use method::COMMAND_METHODS;

pub const DEFAULT_FEED_LIMIT: usize = 10;

pub const COMMAND_NAMES: [&str; 18] = [
    "register", "post", "follow", "like", "feed", "inbox", "mute", "delete", "page", "posts",
    "comment", "reply", "thread", "trends", "stats", "import", "set", "card",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Register(Username),
    Post {
        author: Username,
        text: String,
    },
    Follow {
        who: Username,
        whom: Username,
    },
    Like {
        who: Username,
        post: PostId,
    },
    Feed {
        who: Username,
        limit: usize,
    },
    Inbox(Username),
    Mute {
        who: Username,
        whom: Username,
    },
    Delete {
        who: Username,
        post: PostId,
    },
    Page {
        who: Username,
        number: usize,
        size: usize,
    },
    Posts {
        who: Username,
        author: Username,
        size: usize,
        after: Option<Cursor>,
    },
    Comment {
        who: Username,
        post: PostId,
        text: String,
    },
    Reply {
        who: Username,
        to: CommentId,
        text: String,
    },
    Thread {
        who: Username,
        post: PostId,
    },
    Trends {
        limit: usize,
    },
    Stats,
    Import {
        path: String,
    },
    Set {
        setting: Setting,
        value: u64,
    },
    Card {
        who: Username,
        whose: Username,
    },
    SendMessage {
        from: Username,
        to: Username,
        text: String,
    },
    History {
        who: Username,
        with: Username,
        limit: usize,
    },
}

impl FromStr for Command {
    type Err = ParseError;

    fn from_str(line: &str) -> Result<Command, ParseError> {
        let (name, rest) = split_word(line.trim());
        match name {
            "" => Err(ParseError::Empty),
            "register" => Ok(Command::Register(only_user(rest)?)),
            "inbox" => Ok(Command::Inbox(only_user(rest)?)),
            "post" => {
                let (author, text) = split_word(rest);
                let author = user(author, "ник")?;
                let text = required(text, "текст поста")?.to_string();
                Ok(Command::Post { author, text })
            }
            "follow" => {
                let (who, rest) = split_word(rest);
                let (whom, rest) = split_word(rest);
                let who = user(who, "ник")?;
                let whom = user(whom, "на кого")?;
                no_more(rest)?;
                Ok(Command::Follow { who, whom })
            }
            "like" => {
                let (who, rest) = split_word(rest);
                let (post, rest) = split_word(rest);
                let who = user(who, "ник")?;
                let post = PostId(number(required(post, "id поста")?)?);
                no_more(rest)?;
                Ok(Command::Like { who, post })
            }
            "feed" => {
                let (who, rest) = split_word(rest);
                let (limit, rest) = split_word(rest);
                let who = user(who, "ник")?;
                let limit = number_or(limit, DEFAULT_FEED_LIMIT)?;
                no_more(rest)?;
                Ok(Command::Feed { who, limit })
            }
            "mute" => {
                let (who, rest) = split_word(rest);
                let (whom, rest) = split_word(rest);
                let who = user(who, "ник")?;
                let whom = user(whom, "кого скрыть")?;
                no_more(rest)?;
                Ok(Command::Mute { who, whom })
            }
            "delete" => {
                let (who, rest) = split_word(rest);
                let (post, rest) = split_word(rest);
                let who = user(who, "ник")?;
                let post = PostId(number(required(post, "id поста")?)?);
                no_more(rest)?;
                Ok(Command::Delete { who, post })
            }
            "page" => {
                let (who, rest) = split_word(rest);
                let (page, rest) = split_word(rest);
                let (size, rest) = split_word(rest);
                let who = user(who, "ник")?;
                let page = number(required(page, "номер страницы")?)?;
                let size = number_or(size, DEFAULT_FEED_LIMIT)?;
                no_more(rest)?;
                Ok(Command::Page {
                    who,
                    number: page,
                    size,
                })
            }
            "posts" => {
                let (who, rest) = split_word(rest);
                let (author, rest) = split_word(rest);
                let (size, rest) = split_word(rest);
                let (after, rest) = split_word(rest);
                let who = user(who, "ник")?;
                let author = user(author, "чьи посты")?;
                let size = number_or(size, DEFAULT_FEED_LIMIT)?;
                let after = if after.is_empty() {
                    None
                } else {
                    Some(after.parse()?)
                };
                no_more(rest)?;
                Ok(Command::Posts {
                    who,
                    author,
                    size,
                    after,
                })
            }
            "comment" => {
                let (who, rest) = split_word(rest);
                let (post, text) = split_word(rest);
                let who = user(who, "ник")?;
                let post = PostId(number(required(post, "id поста")?)?);
                let text = required(text, "текст комментария")?.to_string();
                Ok(Command::Comment { who, post, text })
            }
            "reply" => {
                let (who, rest) = split_word(rest);
                let (to, text) = split_word(rest);
                let who = user(who, "ник")?;
                let to = CommentId(number(required(to, "id комментария")?)?);
                let text = required(text, "текст ответа")?.to_string();
                Ok(Command::Reply { who, to, text })
            }
            "thread" => {
                let (who, rest) = split_word(rest);
                let (post, rest) = split_word(rest);
                let who = user(who, "ник")?;
                let post = PostId(number(required(post, "id поста")?)?);
                no_more(rest)?;
                Ok(Command::Thread { who, post })
            }
            "trends" => {
                let (limit, rest) = split_word(rest);
                let limit = number_or(limit, DEFAULT_FEED_LIMIT)?;
                no_more(rest)?;
                Ok(Command::Trends { limit })
            }
            "stats" => {
                no_more(rest)?;
                Ok(Command::Stats)
            }
            "import" => {
                let path = required(rest.trim_end(), "путь к файлу")?.to_string();
                Ok(Command::Import { path })
            }
            "set" => {
                let (setting, rest) = split_word(rest);
                let (value, rest) = split_word(rest);
                let setting = required(setting, "настройка")?.parse()?;
                let value = number(required(value, "значение")?)?;
                no_more(rest)?;
                Ok(Command::Set { setting, value })
            }
            "card" => {
                let (who, rest) = split_word(rest);
                let (whose, rest) = split_word(rest);
                let who = user(who, "ник")?;
                let whose = user(whose, "чья карточка")?;
                no_more(rest)?;
                Ok(Command::Card { who, whose })
            }
            other => Err(ParseError::UnknownCommand(other.to_string())),
        }
    }
}

fn split_word(input: &str) -> (&str, &str) {
    let input = input.trim_start();
    match input.split_once(char::is_whitespace) {
        Some((word, rest)) => (word, rest.trim_start()),
        None => (input, ""),
    }
}

fn required<'a>(word: &'a str, what: &'static str) -> Result<&'a str, ParseError> {
    if word.is_empty() {
        Err(ParseError::MissingArgument(what))
    } else {
        Ok(word)
    }
}

fn user(word: &str, what: &'static str) -> Result<Username, ParseError> {
    Username::try_from(required(word, what)?).map_err(ParseError::BadName)
}

fn only_user(rest: &str) -> Result<Username, ParseError> {
    let (name, rest) = split_word(rest);
    let name = user(name, "ник")?;
    no_more(rest)?;
    Ok(name)
}

fn no_more(rest: &str) -> Result<(), ParseError> {
    match split_word(rest) {
        ("", _) => Ok(()),
        (extra, _) => Err(ParseError::ExtraArgument(extra.to_string())),
    }
}

fn number<T: FromStr>(word: &str) -> Result<T, ParseError> {
    word.parse()
        .map_err(|_| ParseError::BadNumber(word.to_string()))
}

fn number_or(word: &str, default: usize) -> Result<usize, ParseError> {
    if word.is_empty() {
        Ok(default)
    } else {
        number(word)
    }
}
