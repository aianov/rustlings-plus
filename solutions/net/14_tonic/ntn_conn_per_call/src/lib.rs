// Шлюз общается с сервисом пользователей, и на нагрузочном тесте сервис тонет в соединениях.
// Ответы при этом верные. Найди, что шлюз делает не так при каждом вызове, и почини, не потеряв
// параллельность вызовов.
use std::net::SocketAddr;
use tonic::transport::{Channel, Endpoint};
use tonic::Status;

pub mod pb {
    tonic::include_proto!("users.v1");
}
use pb::users_client::UsersClient;
use pb::GetUserRequest;

pub struct Gateway {
    client: UsersClient<Channel>,
}

impl Gateway {
    pub fn new(addr: SocketAddr) -> Self {
        let channel = Endpoint::from_shared(format!("http://{addr}")).expect("адрес корректен").connect_lazy();
        Gateway { client: UsersClient::new(channel) }
    }

    pub async fn handle_of(&self, id: u64) -> Result<String, Status> {
        let mut client = self.client.clone();
        let user = client.get_user(GetUserRequest { id }).await?.into_inner();
        Ok(user.handle)
    }
}
