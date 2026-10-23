// Клиент прислал запрос с Transfer-Encoding: chunked, а значит, длины заранее нет: тело идёт кусками
// "размер в hex, CRLF, данные, CRLF", в конце кусок нулевого размера и трейлеры. Читать надо прямо из
// потока, не зная, сколько байтов будет. Из этого растут две угрозы: клиент заявляет кусок на 4 ГБ, а сервер
// послушно готовит под него буфер; и клиент шлёт строку размера без конца, а read_line читает её до бесконечности.
// TODO: Напиши read_chunked. Лимит max_body на сумму кусков проверяй сразу после чтения размера, до данных.
// Строки (размер, трейлеры) не длиннее MAX_LINE, трейлеров не больше MAX_TRAILERS. Расширение после ';' игнорируй.

use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt};

const MAX_LINE: usize = 256;
const MAX_TRAILERS: usize = 32;

#[derive(Debug, PartialEq)]
enum ChunkError {
    Bad,
    TooLarge,
    Eof,
}

async fn read_chunked<R: AsyncBufRead + Unpin>(reader: &mut R, max_body: usize) -> Result<Vec<u8>, ChunkError> {
    todo!()
}

#[tokio::main]
async fn main() {
    let mut input: &[u8] = b"5\r\nhello\r\n6;note=x\r\n world\r\n0\r\n\r\n";
    println!("{:?}", read_chunked(&mut input, 1024).await.map(|b| String::from_utf8_lossy(&b).into_owned()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::io::{AsyncWriteExt, BufReader, duplex};
    use tokio::time::timeout;

    async fn decode(data: &[u8], max: usize) -> Result<Vec<u8>, ChunkError> {
        let mut input = data;
        read_chunked(&mut input, max).await
    }

    #[tokio::test]
    async fn decodes_chunks_extensions_and_trailers() {
        assert_eq!(decode(b"4\r\nWiki\r\n5\r\npedia\r\nE\r\n in\r\n\r\nchunks.\r\n0\r\n\r\n", 1000).await.unwrap(), b"Wikipedia in\r\n\r\nchunks.");
        assert_eq!(decode(b"a;name=val\r\n0123456789\r\n0\r\nX-Sum: 1\r\nX-Other: 2\r\n\r\n", 100).await.unwrap(), b"0123456789", "расширения и трейлеры пропускаются");
        assert_eq!(decode(b"0\r\n\r\n", 10).await.unwrap(), b"");
    }

    #[tokio::test]
    async fn pieces_may_arrive_one_byte_at_a_time() {
        let (mut client, server) = duplex(8);
        let raw: &[u8] = b"3\r\nabc\r\n2\r\nde\r\n0\r\n\r\n";
        tokio::spawn(async move {
            for b in raw {
                client.write_all(&[*b]).await.unwrap();
                tokio::task::yield_now().await;
            }
        });
        let mut reader = BufReader::new(server);
        let body = timeout(Duration::from_secs(2), read_chunked(&mut reader, 100)).await.expect("завис").unwrap();
        assert_eq!(body, b"abcde");
    }

    #[tokio::test]
    async fn huge_chunk_is_refused_before_its_data() {
        let (mut client, server) = duplex(64);
        client.write_all(b"FFFFFFFF\r\n").await.unwrap();
        let mut reader = BufReader::new(server);
        let result = timeout(Duration::from_secs(2), read_chunked(&mut reader, 1000)).await.expect("сервер ждёт 4 ГБ данных, вместо того чтобы отказать сразу");
        assert_eq!(result, Err(ChunkError::TooLarge));
        let result = decode(b"7FFFFFFFFFFFFFFF\r\n", 1000).await;
        assert_eq!(result, Err(ChunkError::TooLarge), "размер на грани usize не должен переполнять арифметику");
    }

    #[tokio::test]
    async fn limit_applies_to_the_sum() {
        assert_eq!(decode(b"4\r\nabcd\r\n4\r\nefgh\r\n0\r\n\r\n", 8).await.unwrap().len(), 8, "ровно лимит проходит");
        assert_eq!(decode(b"4\r\nabcd\r\n4\r\nefgh\r\n1\r\ni\r\n0\r\n\r\n", 8).await, Err(ChunkError::TooLarge), "лимит на сумму кусков, а не на каждый");
    }

    #[tokio::test]
    async fn malformed_input() {
        for data in [&b"zz\r\nab\r\n0\r\n\r\n"[..], b"\r\n", b"5\r\nhelloXX0\r\n\r\n", b"00000000000000001\r\na\r\n0\r\n\r\n", b"5\nhello\r\n0\r\n\r\n", b"-1\r\n", b"1 \r\na\r\n0\r\n\r\n"] {
            assert_eq!(decode(data, 1000).await, Err(ChunkError::Bad), "{:?} должен быть отвергнут", String::from_utf8_lossy(data));
        }
        assert_eq!(decode(b"5\r\nhel", 1000).await, Err(ChunkError::Eof), "обрыв посреди куска");
        assert_eq!(decode(b"", 1000).await, Err(ChunkError::Eof));
        assert_eq!(decode(b"3\r\nabc\r\n0\r\nX: 1\r\n", 1000).await, Err(ChunkError::Eof), "нет пустой строки после трейлеров");
    }

    #[tokio::test]
    async fn endless_line_is_not_buffered() {
        let big = vec![b'a'; 10_000_000];
        let mut data: &[u8] = &big;
        let mut reader = BufReader::with_capacity(512, &mut data);
        assert_eq!(read_chunked(&mut reader, 1000).await, Err(ChunkError::Bad));
        drop(reader);
        assert!(big.len() - data.len() < 4096, "прочитано {} байт из бесконечной строки: нужно остановиться на MAX_LINE", big.len() - data.len());

        let mut trailers = b"0\r\n".to_vec();
        for i in 0..100 {
            trailers.extend_from_slice(format!("X-{i}: 1\r\n").as_bytes());
        }
        trailers.extend_from_slice(b"\r\n");
        assert_eq!(decode(&trailers, 1000).await, Err(ChunkError::Bad), "число трейлеров тоже ограничено");
    }
}
