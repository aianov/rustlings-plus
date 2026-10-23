use std::fmt;
use std::str::FromStr;

use crate::command::ParseError;

pub const SETTING_NAMES: [&str; 2] = ["post_limit", "trends_window"];

/// ```
/// use contracts::Setting;
///
/// let setting: Setting = "post_limit".parse()?;
/// assert_eq!(setting, Setting::PostLimit);
/// assert_eq!(Setting::TrendsWindow.to_string(), "trends_window");
/// assert!("theme".parse::<Setting>().is_err());
/// # Ok::<(), contracts::ParseError>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Setting {
    PostLimit,
    TrendsWindow,
}

impl Setting {
    pub fn name(self) -> &'static str {
        match self {
            Setting::PostLimit => SETTING_NAMES[0],
            Setting::TrendsWindow => SETTING_NAMES[1],
        }
    }
}

impl fmt::Display for Setting {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl FromStr for Setting {
    type Err = ParseError;

    fn from_str(name: &str) -> Result<Setting, ParseError> {
        match name {
            "post_limit" => Ok(Setting::PostLimit),
            "trends_window" => Ok(Setting::TrendsWindow),
            other => Err(ParseError::UnknownSetting(other.to_string())),
        }
    }
}
