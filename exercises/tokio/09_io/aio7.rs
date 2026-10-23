// Серверная консоль читает команды построчно из любого источника. Пока в тестах одна строка,
// всё хорошо, а на втором вводе команды склеиваются друг с другом. Найди, что в цикле
// живёт дольше, чем нужно. Заодно пусть консоль пропускает пустые строки и понимает как
// `\n`, так и `\r\n`, а последнюю строку без перевода строки не теряет.
use std::io;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};

async fn commands<R: AsyncRead + Unpin>(source: R) -> io::Result<Vec<String>> {
    let mut reader = BufReader::new(source);
    let mut line = String::new();
    let mut out = Vec::new();
    while reader.read_line(&mut line).await? > 0 {
        let command = line.trim_end();
        if !command.is_empty() {
            out.push(command.to_string());
        }
    }
    Ok(out)
}

#[tokio::main]
async fn main() {
    println!("{:?}", commands(&b"ping\nlist\n"[..]).await.unwrap());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn lines_stay_separate() {
        let got = commands(&b"ping\nlist\nquit\n"[..]).await.unwrap();
        assert_eq!(got, vec!["ping", "list", "quit"], "команды склеились: буфер строки нужно очищать между чтениями");
    }

    #[tokio::test]
    async fn crlf_blank_lines_and_no_final_newline() {
        let got = commands(&b"ping\r\n\r\nlist\n\nquit"[..]).await.unwrap();
        assert_eq!(got, vec!["ping", "list", "quit"], "\\r\\n понимаем, пустые строки пропускаем, последнюю без \\n не теряем");
    }

    #[tokio::test]
    async fn empty_input() {
        assert!(commands(&b""[..]).await.unwrap().is_empty(), "пустой ввод это ноль команд");
    }
}
