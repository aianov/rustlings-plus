use crate::common::{GwError, RequestCtx};
use serde_json::Value;
use std::time::Duration;
use tonic::transport::Channel;

pub const CALL_DEADLINE: Duration = Duration::from_millis(300);

pub struct UserGateway {
    channel: Channel,
}

impl UserGateway {
    pub fn new(addr: &str) -> UserGateway {
        todo!()
    }

    pub async fn get_user(&self, data: Value, ctx: &RequestCtx) -> Result<Value, GwError> {
        todo!()
    }
}
