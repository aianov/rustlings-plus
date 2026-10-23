use crate::error::{ContractError, ErrorBody};
use crate::ids::UserId;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, PartialEq, Clone)]
pub struct Request {
    pub id: u64,
    pub service: String,
    pub method: String,
    pub data: Value,
}

#[derive(Debug, PartialEq)]
pub enum Call {
    Register { name: String },
    CreatePost { text: String },
    Follow { user: UserId },
    ChatSend { chat: u64, text: String },
}

#[derive(Debug, PartialEq)]
pub struct Envelope<T> {
    pub id: u64,
    pub data: T,
}

#[derive(Debug, PartialEq)]
pub enum Outcome<T> {
    Ok(T),
    Error(ErrorBody),
}

#[derive(Debug, PartialEq)]
pub struct Response<T> {
    pub id: u64,
    pub outcome: Outcome<T>,
}

pub fn parse_request(raw: &str) -> Result<Request, ContractError> {
    todo!()
}

pub fn decode_call(request: &Request) -> Result<Call, ContractError> {
    todo!()
}

pub fn parse_call(raw: &str) -> Result<(u64, Call), ContractError> {
    todo!()
}

pub fn encode<T: Serialize>(id: u64, data: &T) -> String {
    todo!()
}

pub fn decode<T: DeserializeOwned>(raw: &str) -> Result<Envelope<T>, ContractError> {
    todo!()
}
