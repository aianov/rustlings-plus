#[tokio::main]
async fn main() -> Result<(), String> {
    echo_service::run().await
}
