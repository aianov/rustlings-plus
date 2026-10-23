// Шлюз общается с сервисом пользователей, и на нагрузочном тесте сервис тонет в соединениях.
// Ответы при этом верные. Найди, что шлюз делает не так при каждом вызове, и почини, не потеряв
// параллельность вызовов.
use std::net::SocketAddr;
use tonic::Status;

pub mod pb {
    tonic::include_proto!("users.v1");
}
use pb::users_client::UsersClient;
use pb::GetUserRequest;

pub struct Gateway {
    addr: SocketAddr,
}

impl Gateway {
    pub fn new(addr: SocketAddr) -> Self {
        Gateway { addr }
    }

    pub async fn handle_of(&self, id: u64) -> Result<String, Status> {
        let mut client = UsersClient::connect(format!("http://{}", self.addr)).await.map_err(|e| Status::unavailable(e.to_string()))?;
        let user = client.get_user(GetUserRequest { id }).await?.into_inner();
        Ok(user.handle)
    }
}
