mod error;

use std::str::FromStr;

use crate::ids::PostId;
use crate::username::Username;

pub use error::ParseError;

pub const DEFAULT_FEED_LIMIT: usize = 10;

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
                let limit = if limit.is_empty() {
                    DEFAULT_FEED_LIMIT
                } else {
                    number(limit)?
                };
                no_more(rest)?;
                Ok(Command::Feed { who, limit })
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
