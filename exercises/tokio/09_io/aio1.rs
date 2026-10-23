// Сервис отчётов получает данные как поток байтов и отдаёт их так же. Тут два помощника:
// один вычитывает источник целиком, другой пишет приветствие в приёмник. Компилятор уверяет,
// что у источника нет метода `read_to_end`, хотя тип точно умеет читать, а с `write_all`
// та же история. Разберись, почему, и добейся, чтобы тесты прошли.
use tokio::io::{AsyncRead, AsyncWrite};

async fn slurp<R: AsyncRead + Unpin>(mut source: R) -> std::io::Result<Vec<u8>> {
    let mut data = Vec::new();
    source.read_to_end(&mut data).await?;
    Ok(data)
}

async fn greet<W: AsyncWrite + Unpin>(mut sink: W, name: &str) -> std::io::Result<()> {
    sink.write_all(b"hello, ").await?;
    sink.write_all(name.as_bytes()).await?;
    sink.write_all(b"\n").await
}

#[tokio::main]
async fn main() {
    let mut out = Vec::new();
    greet(&mut out, "tokio").await.unwrap();
    print!("{}", String::from_utf8_lossy(&out));
    println!("{}", slurp(&out[..]).await.unwrap().len());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn slurp_reads_everything() {
        let data = slurp(&b"abc\ndef"[..]).await.unwrap();
        assert_eq!(data, b"abc\ndef", "slurp должен вернуть все байты источника");
    }

    #[tokio::test]
    async fn greet_writes_all_parts() {
        let mut out = Vec::new();
        greet(&mut out, "Ann").await.unwrap();
        assert_eq!(out, b"hello, Ann\n", "в приёмник должна попасть вся строка приветствия");
    }
}
