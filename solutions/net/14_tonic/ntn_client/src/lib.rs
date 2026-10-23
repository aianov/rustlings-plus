// Шлюзу нужен канал до сервиса пользователей: его создают при старте, а сервис в этот момент
// может быть ещё не поднят. Вызов не должен висеть вечно. Допиши make_channel.
use std::net::SocketAddr;
use std::time::Duration;
use tonic::transport::{Channel, Endpoint};
use tonic::Status;

pub mod pb {
    tonic::include_proto!("users.v1");
}
use pb::users_client::UsersClient;
use pb::GetUserRequest;

pub fn make_channel(addr: SocketAddr, connect_timeout: Duration, request_timeout: Duration) -> Channel {
    Endpoint::from_shared(format!("http://{addr}"))
        .expect("адрес собран из SocketAddr и всегда корректен")
        .connect_timeout(connect_timeout)
        .timeout(request_timeout)
        .connect_lazy()
}

pub async fn handle_of(channel: Channel, id: u64) -> Result<String, Status> {
    let mut client = UsersClient::new(channel);
    let user = client.get_user(GetUserRequest { id }).await?.into_inner();
    Ok(user.handle)
}
