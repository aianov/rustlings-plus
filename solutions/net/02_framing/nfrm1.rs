use std::io;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, BufReader};

async fn read_command<R: AsyncBufRead + Unpin>(reader: &mut R) -> io::Result<Option<String>> {
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).await? == 0 {
            return Ok(None);
        }
        let Some(line) = line.strip_suffix('\n') else {
            return Err(io::ErrorKind::UnexpectedEof.into());
        };
        let command = line.strip_suffix('\r').unwrap_or(line);
        if !command.is_empty() {
            return Ok(Some(command.to_owned()));
        }
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let mut input = BufReader::new(tokio::io::stdin());
    while let Some(cmd) = read_command(&mut input).await? {
        println!("команда: {cmd}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::io::{AsyncWriteExt, duplex};
    use tokio::time::timeout;

    async fn all(input: &'static [u8]) -> io::Result<Vec<String>> {
        let mut reader = BufReader::new(input);
        let mut out = Vec::new();
        while let Some(c) = read_command(&mut reader).await? {
            out.push(c);
        }
        Ok(out)
    }

    #[tokio::test]
    async fn plain_and_windows_line_endings() {
        assert_eq!(all(b"PING\nECHO hi\r\nQUIT\n").await.unwrap(), ["PING", "ECHO hi", "QUIT"], "'\\n' и '\\r\\n' должны давать чистую команду");
    }

    #[tokio::test]
    async fn empty_lines_are_skipped() {
        assert_eq!(all(b"\n\r\nPING\n\n").await.unwrap(), ["PING"], "пустые строки не команды");
    }

    #[tokio::test]
    async fn spaces_are_content() {
        assert_eq!(all(b"  \n").await.unwrap(), ["  "], "строка из пробелов это не пустая строка, это команда как есть");
    }

    #[tokio::test]
    async fn unterminated_tail_is_an_error() {
        let err = all(b"PING\nQUI").await.expect_err("«QUI» без перевода строки: клиент оборвался, это не команда");
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof, "ждали UnexpectedEof, получили {err}");
    }

    #[tokio::test]
    async fn bad_utf8_is_invalid_data() {
        let err = all(b"\xff\xfe\n").await.expect_err("не UTF-8");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData, "ждали InvalidData, получили {err}");
    }

    #[tokio::test]
    async fn waits_for_slow_writer() {
        let (mut client, server) = duplex(64);
        let task = tokio::spawn(async move {
            let mut r = BufReader::new(server);
            read_command(&mut r).await
        });
        client.write_all(b"PI").await.unwrap();
        tokio::time::sleep(Duration::from_millis(30)).await;
        client.write_all(b"NG\r").await.unwrap();
        tokio::time::sleep(Duration::from_millis(30)).await;
        client.write_all(b"\n").await.unwrap();
        let got = timeout(Duration::from_secs(3), task).await.expect("завис").unwrap().unwrap();
        assert_eq!(got.as_deref(), Some("PING"), "команда пришла тремя кусками");
    }
}
