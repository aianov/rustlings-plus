use tokio::sync::{mpsc, oneshot};
use tokio::time::timeout;

use crate::codec::RespError;
use crate::command::{Cmd, cmd};
use crate::config::RedisConfig;
use crate::conn::{Request, spawn};
use crate::value::Value;

#[derive(Clone)]
pub struct Client {
    tx: mpsc::Sender<Request>,
    config: RedisConfig,
}

fn checked(value: Value) -> Result<Value, RespError> {
    match value {
        Value::Error(why) => Err(RespError::Server(why)),
        other => Ok(other),
    }
}

impl Client {
    pub fn connect(config: RedisConfig) -> Client {
        Client {
            tx: spawn(config.clone()),
            config,
        }
    }

    pub fn dedicated(&self) -> Client {
        Client::connect(self.config.clone())
    }

    pub fn config(&self) -> &RedisConfig {
        &self.config
    }

    pub async fn pipeline(&self, commands: Vec<Cmd>) -> Result<Vec<Value>, RespError> {
        if commands.is_empty() {
            return Ok(Vec::new());
        }
        let (reply, answer) = oneshot::channel();
        let request = Request { commands, reply };
        self.tx.send(request).await.map_err(|_| RespError::Closed)?;
        match timeout(self.config.timeout, answer).await {
            Err(..) => Err(RespError::Timeout),
            Ok(Err(..)) => Err(RespError::Closed),
            Ok(Ok(replies)) => replies,
        }
    }

    pub async fn query(&self, command: Cmd) -> Result<Value, RespError> {
        let mut replies = self.pipeline(vec![command]).await?;
        checked(replies.pop().ok_or(RespError::Closed)?)
    }

    pub async fn ping(&self) -> Result<(), RespError> {
        match self.query(cmd("PING")).await? {
            Value::Simple(pong) if pong == "PONG" => Ok(()),
            other => Err(RespError::BadFrame(format!("на PING пришло {other:?}"))),
        }
    }

    pub async fn get(&self, key: &str) -> Result<Option<String>, RespError> {
        Ok(self.query(cmd("GET").arg(key)).await?.as_text())
    }

    pub async fn set_ex(&self, key: &str, value: &str, seconds: u64) -> Result<(), RespError> {
        self.query(cmd("SET").arg(key).arg(value).arg("EX").arg(seconds))
            .await?;
        Ok(())
    }

    pub async fn incr(&self, key: &str) -> Result<i64, RespError> {
        let value = self.query(cmd("INCR").arg(key)).await?;
        value
            .as_int()
            .ok_or_else(|| RespError::BadFrame(String::from("INCR")))
    }

    pub async fn del(&self, keys: &[String]) -> Result<i64, RespError> {
        let value = self.query(cmd("DEL").args(keys)).await?;
        Ok(value.as_int().unwrap_or(0))
    }

    pub async fn expire(&self, key: &str, seconds: u64) -> Result<bool, RespError> {
        let value = self.query(cmd("EXPIRE").arg(key).arg(seconds)).await?;
        Ok(value.as_int() == Some(1))
    }

    pub async fn pfadd(&self, key: &str, items: &[String]) -> Result<bool, RespError> {
        let value = self.query(cmd("PFADD").arg(key).args(items)).await?;
        Ok(value.as_int() == Some(1))
    }

    pub async fn pfcount(&self, key: &str) -> Result<u64, RespError> {
        let value = self.query(cmd("PFCOUNT").arg(key)).await?;
        Ok(u64::try_from(value.as_int().unwrap_or(0)).unwrap_or(0))
    }

    pub async fn lrange(&self, key: &str, start: i64, stop: i64) -> Result<Vec<String>, RespError> {
        let value = self
            .query(cmd("LRANGE").arg(key).arg(start).arg(stop))
            .await?;
        Ok(value.items().iter().filter_map(Value::as_text).collect())
    }

    pub async fn zrevrange_scores(
        &self,
        key: &str,
        limit: usize,
    ) -> Result<Vec<(String, f64)>, RespError> {
        let stop = i64::try_from(limit).unwrap_or(i64::MAX) - 1;
        let value = self
            .query(cmd("ZREVRANGE").arg(key).arg(0).arg(stop).arg("WITHSCORES"))
            .await?;
        let items = value.items();
        Ok(items
            .chunks(2)
            .filter_map(|pair| match pair {
                [member, score] => Some((member.as_text()?, score.as_text()?.parse().ok()?)),
                _ => None,
            })
            .collect())
    }

    pub async fn blpop(&self, key: &str, seconds: u64) -> Result<Option<String>, RespError> {
        let value = self.query(cmd("BLPOP").arg(key).arg(seconds)).await?;
        Ok(value.items().get(1).and_then(Value::as_text))
    }
}
