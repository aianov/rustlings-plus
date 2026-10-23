use validation::{CharLen, Matches, NotBlank, Rule, RuleExt, Violation};

use super::{MAX_COMMENT_LEN, MAX_POST_LEN};

pub const MAX_TAGS: usize = 5;

pub const MAX_TAG_LEN: usize = 30;

pub const MAX_LINKS: usize = 3;

pub const MAX_LINK_TITLE: usize = 80;

pub const TAG_PATTERN: Matches = Matches {
    pattern: "[a-z0-9_]+",
    what: "латиницу, цифры и _",
};

fn text(raw: &str, max: usize) -> Result<String, Violation> {
    let text = raw.trim();
    NotBlank.and(CharLen { min: 1, max }).check(text)?;
    Ok(text.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostText(String);

impl TryFrom<&str> for PostText {
    type Error = Violation;

    fn try_from(raw: &str) -> Result<PostText, Violation> {
        text(raw, MAX_POST_LEN).map(PostText)
    }
}

impl AsRef<str> for PostText {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentText(String);

impl TryFrom<&str> for CommentText {
    type Error = Violation;

    fn try_from(raw: &str) -> Result<CommentText, Violation> {
        text(raw, MAX_COMMENT_LEN).map(CommentText)
    }
}

impl AsRef<str> for CommentText {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag(String);

impl TryFrom<&str> for Tag {
    type Error = Violation;

    fn try_from(raw: &str) -> Result<Tag, Violation> {
        let tag = raw.trim().trim_start_matches('#').to_lowercase();
        let rule = CharLen {
            min: 1,
            max: MAX_TAG_LEN,
        }
        .and(TAG_PATTERN);
        rule.check(tag.as_str())?;
        Ok(Tag(tag))
    }
}

impl AsRef<str> for Tag {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub url: String,
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    pub text: PostText,
    pub tags: Vec<Tag>,
    pub links: Vec<Link>,
}
