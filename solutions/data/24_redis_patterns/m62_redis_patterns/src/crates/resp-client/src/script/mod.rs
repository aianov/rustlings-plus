use std::sync::{Mutex, PoisonError};

use crate::client::Client;
use crate::codec::RespError;
use crate::command::cmd;
use crate::value::Value;

pub struct Script {
    source: &'static str,
    sha: Mutex<Option<String>>,
}

impl Script {
    pub const fn new(source: &'static str) -> Script {
        Script {
            source,
            sha: Mutex::new(None),
        }
    }

    pub fn source(&self) -> &'static str {
        self.source
    }

    fn known(&self) -> Option<String> {
        self.sha
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn remember(&self, sha: String) {
        *self.sha.lock().unwrap_or_else(PoisonError::into_inner) = Some(sha);
    }

    fn forget(&self) {
        *self.sha.lock().unwrap_or_else(PoisonError::into_inner) = None;
    }
}

impl Client {
    pub async fn load(&self, script: &Script) -> Result<String, RespError> {
        let sha = self
            .query(cmd("SCRIPT").arg("LOAD").arg(script.source))
            .await?
            .as_text()
            .ok_or_else(|| RespError::BadFrame(String::from("SCRIPT LOAD без sha")))?;
        script.remember(sha.clone());
        Ok(sha)
    }

    pub async fn eval(
        &self,
        script: &Script,
        keys: &[String],
        args: &[String],
    ) -> Result<Value, RespError> {
        let sha = match script.known() {
            Some(sha) => sha,
            None => self.load(script).await?,
        };
        let by_sha = cmd("EVALSHA")
            .arg(&sha)
            .arg(keys.len())
            .args(keys)
            .args(args);
        match self.query(by_sha).await {
            Err(RespError::Server(why)) if why.starts_with("NOSCRIPT") => {
                script.forget();
                let whole = cmd("EVAL")
                    .arg(script.source)
                    .arg(keys.len())
                    .args(keys)
                    .args(args);
                let value = self.query(whole).await?;
                self.load(script).await?;
                Ok(value)
            }
            other => other,
        }
    }
}
