// Когда у процесса кончаются файловые дескрипторы (EMFILE), accept возвращает ошибку. Это
// временная беда: клиенты уйдут, дескрипторы освободятся. Сервер, который из-за неё выходит,
// ложится целиком, а сервер, который тут же зовёт accept снова, крутит процессор вхолостую.
// TODO: Допиши is_temporary (какие ошибки ждём) и accept_loop: при временной ошибке пауза backoff
// и дальше, при остальных вернуть ошибку наружу. Каждое принятое соединение отдай в on_conn.

use std::io;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};

trait Accept {
    type Conn;
    async fn accept(&mut self) -> io::Result<Self::Conn>;
}

impl Accept for TcpListener {
    type Conn = TcpStream;

    async fn accept(&mut self) -> io::Result<TcpStream> {
        Ok(TcpListener::accept(self).await?.0)
    }
}

fn is_temporary(e: &io::Error) -> bool {
    todo!()
}

async fn accept_loop<A: Accept>(acceptor: &mut A, backoff: Duration, mut on_conn: impl FnMut(A::Conn)) -> io::Result<()> {
    todo!()
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let mut listener = TcpListener::bind("127.0.0.1:0").await?;
    println!("слушаем {}", listener.local_addr()?);
    accept_loop(&mut listener, Duration::from_millis(100), |stream| {
        tokio::spawn(async move { drop(stream) });
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::io::ErrorKind;
    use tokio::time::Instant;

    struct Fake {
        script: VecDeque<io::Result<u32>>,
    }

    impl Accept for Fake {
        type Conn = u32;

        async fn accept(&mut self) -> io::Result<u32> {
            self.script.pop_front().unwrap_or_else(|| Err(io::Error::from(ErrorKind::PermissionDenied)))
        }
    }

    fn emfile() -> io::Error {
        io::Error::from_raw_os_error(24)
    }

    #[test]
    fn classifies_errors() {
        assert!(is_temporary(&emfile()), "EMFILE (24, нет дескрипторов) временная ошибка");
        assert!(is_temporary(&io::Error::from_raw_os_error(23)), "ENFILE (23, нет дескрипторов в системе) временная ошибка");
        assert!(is_temporary(&io::Error::from(ErrorKind::ConnectionAborted)), "клиент ушёл, не дождавшись accept: это не повод падать");
        assert!(!is_temporary(&io::Error::from(ErrorKind::PermissionDenied)), "PermissionDenied сама не пройдёт");
        assert!(!is_temporary(&io::Error::from(ErrorKind::InvalidInput)), "InvalidInput сама не пройдёт");
    }

    #[tokio::test(start_paused = true)]
    async fn keeps_going_after_temporary_errors() {
        let mut fake = Fake {
            script: VecDeque::from([Err(emfile()), Ok(1), Err(emfile()), Err(emfile()), Ok(2)]),
        };
        let mut got = Vec::new();
        let err = accept_loop(&mut fake, Duration::from_millis(50), |c| got.push(c))
            .await
            .expect_err("в конце скрипта фатальная ошибка, цикл должен её вернуть");
        assert_eq!(err.kind(), ErrorKind::PermissionDenied, "наружу должна выйти фатальная ошибка, а не другая");
        assert_eq!(got, vec![1, 2], "каждое успешное соединение должно дойти до on_conn, временные ошибки пропускаются");
    }

    #[tokio::test(start_paused = true)]
    async fn pauses_instead_of_spinning() {
        let mut fake = Fake {
            script: VecDeque::from([Err(emfile()), Err(emfile()), Err(emfile())]),
        };
        let started = Instant::now();
        let _ = accept_loop(&mut fake, Duration::from_millis(50), |_| {}).await;
        assert!(
            started.elapsed() >= Duration::from_millis(150),
            "три временные ошибки подряд, а пауз на 150 мс не было: горячий цикл жжёт процессор"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn fatal_error_leaves_at_once() {
        let mut fake = Fake { script: VecDeque::new() };
        let mut calls = 0;
        let started = Instant::now();
        let err = accept_loop(&mut fake, Duration::from_secs(10), |_| calls += 1).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::PermissionDenied, "ожидали ошибку из accept");
        assert_eq!(calls, 0, "соединений не было");
        assert!(started.elapsed() < Duration::from_secs(1), "на фатальной ошибке паузы быть не должно");
    }

    #[tokio::test]
    async fn works_with_a_real_listener() {
        let mut listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let _client = TcpStream::connect(addr).await.unwrap();
        let conn = tokio::time::timeout(Duration::from_secs(3), Accept::accept(&mut listener)).await.expect("завис").unwrap();
        assert_eq!(conn.peer_addr().unwrap(), _client.local_addr().unwrap(), "принят не тот клиент");
    }
}
