#![allow(unused_variables, dead_code, unused_imports)]

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use scylla::Session;
use serde_json::{Value, json};
use sqlx::PgPool;

pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub id: i64,
    pub aggregate_id: i64,
    pub kind: String,
    pub payload: Value,
}

pub trait Sink: Send + Sync {
    fn publish(&self, event: &Event) -> Result<(), String>;
}

pub struct Pipeline {
    pg: PgPool,
    scylla: Arc<Session>,
    keyspace: String,
}

fn now_us() -> i64 {
    let us = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_micros()).unwrap_or(0);
    i64::try_from(us).unwrap_or(i64::MAX)
}

impl Pipeline {
    pub async fn new(pg: PgPool, scylla: Arc<Session>, keyspace: &str) -> Result<Pipeline, BoxError> {
        todo!("таблицы в Postgres и таблица feed в Scylla, ожидание согласования схемы")
    }

    pub async fn follow(&self, follower: i64, author: i64) -> Result<(), BoxError> {
        todo!()
    }

    pub async fn create_post(&self, author: i64, text: &str) -> Result<i64, BoxError> {
        todo!("пост и событие post_created одной транзакцией, версия now_us()")
    }

    pub async fn delete_post(&self, post: i64) -> Result<(), BoxError> {
        todo!("отметка deleted и событие post_deleted одной транзакцией")
    }

    pub async fn relay_once(&self, sink: &dyn Sink, batch: i64) -> Result<usize, BoxError> {
        todo!("пачка с SKIP LOCKED и порядком по агрегату, отметка только опубликованных")
    }

    pub async fn consume(&self, event: &Event) -> Result<bool, BoxError> {
        todo!("inbox, лента в Scylla с USING TIMESTAMP версии, коммит inbox")
    }

    pub async fn feed(&self, user: i64) -> Result<Vec<(i64, String)>, BoxError> {
        todo!()
    }
}
