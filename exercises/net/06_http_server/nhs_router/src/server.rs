use std::io;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;

use crate::{Limits, Method, Parsed, Request, Response, Router, parse_request};

pub async fn serve(listener: TcpListener, router: Router) -> io::Result<()> {
    todo!()
}

async fn run_handler(router: Arc<Router>, request: Request) -> Response {
    todo!()
}

async fn connection(stream: TcpStream, router: Arc<Router>) {
    todo!()
}

// Закрыть соединение после ответа об ошибке так, чтобы клиент этот ответ получил (см. README главы,
// раздел «Не теряй ответ об ошибке»). Запись закрывается, остаток входа вычитывается с пределом.
async fn finish(_stream: TcpStream) {
    todo!()
}
