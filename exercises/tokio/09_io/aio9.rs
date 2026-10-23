// Отчёт пишется построчно в любой приёмник: файл, сокет, буфер в памяти. Чтобы не дёргать
// систему на каждую короткую строку, запись идёт через `BufWriter`. Функция отрабатывает без
// ошибок, а в приёмнике оказывается пусто. Куда пропали строки?
use std::io;
use tokio::io::{AsyncWrite, AsyncWriteExt, BufWriter};

async fn write_table<W: AsyncWrite + Unpin>(sink: &mut W, rows: &[(&str, u32)]) -> io::Result<()> {
    let mut out = BufWriter::new(sink);
    for (name, count) in rows {
        out.write_all(format!("{name}={count}\n").as_bytes()).await?;
    }
    Ok(())
}

#[tokio::main]
async fn main() {
    let mut out = Vec::new();
    write_table(&mut out, &[("posts", 3)]).await.unwrap();
    print!("{}", String::from_utf8_lossy(&out));
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{duplex, AsyncReadExt};

    #[tokio::test]
    async fn rows_reach_the_vec() {
        let mut out = Vec::new();
        write_table(&mut out, &[("posts", 3), ("likes", 12)]).await.unwrap();
        assert_eq!(out, b"posts=3\nlikes=12\n", "после возврата из функции все строки должны быть в приёмнике");
    }

    #[tokio::test]
    async fn rows_reach_the_peer() {
        let (mut client, mut server) = duplex(1024);
        write_table(&mut client, &[("a", 1)]).await.unwrap();
        drop(client);
        let mut got = String::new();
        server.read_to_string(&mut got).await.unwrap();
        assert_eq!(got, "a=1\n", "собеседник ничего не получил: данные застряли в буфере");
    }
}
