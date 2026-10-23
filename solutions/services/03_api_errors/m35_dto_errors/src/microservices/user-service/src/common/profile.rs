use validation::{CharLen, NoControl, NotBlank, Rule, RuleExt, Violation, collapse_spaces};

use super::User;

pub const MAX_DISPLAY_NAME: usize = 40;

pub const MAX_BIO: usize = 160;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayName(String);

impl TryFrom<&str> for DisplayName {
    type Error = Violation;

    fn try_from(raw: &str) -> Result<DisplayName, Violation> {
        let name = collapse_spaces(raw);
        let rule = NotBlank.and(NoControl).and(CharLen {
            min: 1,
            max: MAX_DISPLAY_NAME,
        });
        rule.check(name.as_str())?;
        Ok(DisplayName(name))
    }
}

impl AsRef<str> for DisplayName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bio(String);

fn no_control_but_newlines(text: &str) -> Result<(), Violation> {
    NoControl.check(text.replace('\n', " ").as_str())
}

impl TryFrom<&str> for Bio {
    type Error = Violation;

    fn try_from(raw: &str) -> Result<Bio, Violation> {
        let bio = raw.trim().replace("\r\n", "\n");
        let rule = NotBlank.and(no_control_but_newlines).and(CharLen {
            min: 1,
            max: MAX_BIO,
        });
        rule.check(bio.as_str())?;
        Ok(Bio(bio))
    }
}

impl AsRef<str> for Bio {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Patch<T> {
    #[default]
    Keep,
    Clear,
    Set(T),
}

impl<T: Clone> Patch<T> {
    pub fn apply(&self, current: &Option<T>) -> Option<T> {
        match self {
            Patch::Keep => current.clone(),
            Patch::Clear => None,
            Patch::Set(value) => Some(value.clone()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProfileChange {
    pub display_name: Patch<DisplayName>,
    pub bio: Patch<Bio>,
}

impl ProfileChange {
    pub fn apply(&self, user: &User) -> User {
        User {
            display_name: self.display_name.apply(&user.display_name),
            bio: self.bio.apply(&user.bio),
            ..user.clone()
        }
    }
}
