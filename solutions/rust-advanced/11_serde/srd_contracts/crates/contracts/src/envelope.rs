use crate::error::{ContractError, ErrorBody, reason_of};
use crate::ids::UserId;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
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

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Envelope<T> {
    pub id: u64,
    pub data: T,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome<T> {
    Ok(T),
    Error(ErrorBody),
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Response<T> {
    pub id: u64,
    #[serde(flatten)]
    pub outcome: Outcome<T>,
}

#[derive(Deserialize)]
struct RegisterData {
    name: String,
}

#[derive(Deserialize)]
struct CreatePostData {
    text: String,
}

#[derive(Deserialize)]
struct FollowData {
    user: UserId,
}

#[derive(Deserialize)]
struct ChatSendData {
    chat: u64,
    text: String,
}

pub fn parse_request(raw: &str) -> Result<Request, ContractError> {
    serde_json::from_str(raw).map_err(|e| ContractError::from_json(&e))
}

fn data_of<'a, T: Deserialize<'a>>(request: &'a Request) -> Result<T, ContractError> {
    T::deserialize(&request.data)
        .map_err(|e| ContractError::BadData { method: request.method.to_string(), reason: reason_of(&e) })
}

pub fn decode_call(request: &Request) -> Result<Call, ContractError> {
    match request.method.as_str() {
        "users.register" => Ok(Call::Register { name: data_of::<RegisterData>(request)?.name }),
        "posts.create" => Ok(Call::CreatePost { text: data_of::<CreatePostData>(request)?.text }),
        "users.follow" => Ok(Call::Follow { user: data_of::<FollowData>(request)?.user }),
        "chat.send" => {
            let data: ChatSendData = data_of(request)?;
            Ok(Call::ChatSend { chat: data.chat, text: data.text })
        }
        other => Err(ContractError::UnknownMethod(other.to_string())),
    }
}

pub fn parse_call(raw: &str) -> Result<(u64, Call), ContractError> {
    let request = parse_request(raw)?;
    let call = decode_call(&request)?;
    Ok((request.id, call))
}

pub fn encode<T: Serialize>(id: u64, data: &T) -> String {
    serde_json::to_string(&Envelope { id, data }).expect("конверт с обычными данными сериализуется")
}

pub fn decode<T: DeserializeOwned>(raw: &str) -> Result<Envelope<T>, ContractError> {
    serde_json::from_str(raw).map_err(|e| ContractError::from_json(&e))
}
