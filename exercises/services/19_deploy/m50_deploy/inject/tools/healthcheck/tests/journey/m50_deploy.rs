use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Duration;

use healthcheck::{ProbeError, probe};

fn answer(reply: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("порт");
    let addr = listener.local_addr().expect("адрес");
    std::thread::spawn(move || {
        if let Ok((mut socket, _)) = listener.accept() {
            let mut buf = [0u8; 1024];
            let _ = socket.read(&mut buf);
            let _ = socket.write_all(reply.as_bytes());
        }
    });
    format!("http://{addr}/ready")
}

#[test]
fn ready_is_200_and_nothing_else() {
    let limit = Duration::from_secs(2);
    assert_eq!(
        probe(&answer("HTTP/1.1 200 OK\r\n\r\nready"), limit).ok(),
        Some(200)
    );
    assert!(matches!(
        probe(&answer("HTTP/1.1 503 Service Unavailable\r\n\r\n"), limit),
        Err(ProbeError::Status(503))
    ));
    assert!(matches!(
        probe(&answer("мусор"), limit),
        Err(ProbeError::Io(..))
    ));
    assert!(matches!(
        probe("ftp://127.0.0.1:1/", limit),
        Err(ProbeError::Url(..))
    ));
    assert!(matches!(
        probe("http://127.0.0.1:1/ready", limit),
        Err(ProbeError::Connect(..))
    ));
}
