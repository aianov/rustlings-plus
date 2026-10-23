use contracts::Setting;
use tokio::sync::watch;

use super::{App, AppError};
use crate::domain::MAX_POST_LEN;
use crate::ports::{PostStore, UserStore};
use crate::trends::TRENDS_WINDOW;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    pub post_limit: usize,
    pub trends_window: u64,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            post_limit: MAX_POST_LEN,
            trends_window: TRENDS_WINDOW,
        }
    }
}

fn allowed(setting: Setting, value: u64) -> bool {
    match setting {
        Setting::PostLimit => (1..=MAX_POST_LEN as u64).contains(&value),
        Setting::TrendsWindow => value >= 1,
    }
}

impl<U, P> App<U, P>
where
    U: UserStore + Send + 'static,
    P: PostStore + Send + 'static,
{
    pub fn settings(&self) -> Settings {
        *self.inner.settings.borrow()
    }

    pub fn watch_settings(&self) -> watch::Receiver<Settings> {
        self.inner.settings.subscribe()
    }

    pub fn set(&self, setting: Setting, value: u64) -> Result<Settings, AppError> {
        if !allowed(setting, value) {
            return Err(AppError::BadSetting { setting, value });
        }
        self.inner.settings.send_modify(|settings| match setting {
            Setting::PostLimit => settings.post_limit = value as usize,
            Setting::TrendsWindow => settings.trends_window = value,
        });
        Ok(self.settings())
    }
}
