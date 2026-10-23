use crate::common::{GwError, RequestCtx};
use crate::pb::GetUserRequest;
use crate::pb::users_client::UsersClient;
use serde_json::{Value, json};
use std::time::Duration;
use tonic::metadata::MetadataValue;
use tonic::transport::{Channel, Endpoint};
use tonic::{Code, Request, Status};

pub const CALL_DEADLINE: Duration = Duration::from_millis(300);

pub struct UserGateway {
    channel: Channel,
}

fn to_gw_error(status: &Status) -> GwError {
    match status.code() {
        Code::NotFound => GwError::new("not_found", "пользователь не найден"),
        Code::InvalidArgument => GwError::new("invalid_argument", "данные запроса не подходят"),
        Code::Unavailable => GwError::new("unavailable", "сервис временно недоступен"),
        Code::DeadlineExceeded | Code::Cancelled => GwError::new("deadline_exceeded", "сервис не успел ответить"),
        _ => GwError::new("internal", "внутренняя ошибка"),
    }
}

impl UserGateway {
    pub fn new(addr: &str) -> UserGateway {
        let channel = Endpoint::from_shared(format!("http://{addr}")).expect("адрес сервиса").connect_lazy();
        UserGateway { channel }
    }

    pub async fn get_user(&self, data: Value, ctx: &RequestCtx) -> Result<Value, GwError> {
        let id = data.get("id").and_then(Value::as_u64).ok_or_else(|| GwError::new("invalid_argument", "нужен числовой id"))?;
        let mut request = Request::new(GetUserRequest { id });
        request.set_timeout(CALL_DEADLINE);
        if let Ok(value) = MetadataValue::try_from(ctx.request_id.as_str()) {
            request.metadata_mut().insert("x-request-id", value);
        }
        if let Some(Ok(value)) = ctx.user_id.as_deref().map(MetadataValue::try_from) {
            request.metadata_mut().insert("x-user-id", value);
        }
        let mut client = UsersClient::new(self.channel.clone());
        let user = client.get_user(request).await.map_err(|status| to_gw_error(&status))?.into_inner();
        Ok(json!({"id": user.id, "name": user.name}))
    }
}
