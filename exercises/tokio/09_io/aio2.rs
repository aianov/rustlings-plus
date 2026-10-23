// Сеть отдаёт байты как придётся: одно сообщение может прийти двумя кусками, два сообщения
// одним. Прежде чем писать протоколы, стоит один раз увидеть это своими глазами.
// TODO: Допиши наблюдателя `chunk_sizes`: он читает источник буфером на `cap` байт до конца
// потока и возвращает размер каждого прочитанного куска по порядку. Куски нулевого размера
// в ответ не попадают.
use std::io;
use tokio::io::{AsyncRead, AsyncReadExt};

async fn chunk_sizes<R: AsyncRead + Unpin>(mut source: R, cap: usize) -> io::Result<Vec<usize>> {
    let _ = (&mut source, cap);
    todo!()
}

#[tokio::main]
async fn main() {
    println!("{:?}", chunk_sizes(&b"hello world"[..], 4).await.unwrap());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::io::{duplex, AsyncWriteExt, DuplexStream};
    use tokio::time::sleep;

    fn chunked(parts: &'static [&'static str]) -> DuplexStream {
        let (mut tx, rx) = duplex(64);
        tokio::spawn(async move {
            for part in parts {
                tx.write_all(part.as_bytes()).await.unwrap();
                sleep(Duration::from_millis(10)).await;
            }
        });
        rx
    }

    #[tokio::test(start_paused = true)]
    async fn sizes_follow_the_writes() {
        let sizes = chunk_sizes(chunked(&["ab", "cde", "f"]), 16).await.unwrap();
        assert_eq!(sizes, vec![2, 3, 1], "каждая запись отправителя должна прийти отдельным куском");
    }

    #[tokio::test(start_paused = true)]
    async fn small_buffer_splits_one_write() {
        let sizes = chunk_sizes(chunked(&["abcdefgh"]), 3).await.unwrap();
        assert_eq!(sizes, vec![3, 3, 2], "буфер на 3 байта режет запись на куски 3, 3 и 2");
    }

    #[tokio::test]
    async fn slice_source() {
        let sizes = chunk_sizes(&b"abcdef"[..], 4).await.unwrap();
        assert_eq!(sizes, vec![4, 2], "срез отдаёт данные кусками по размеру буфера");
    }

    #[tokio::test]
    async fn empty_source_has_no_chunks() {
        let sizes = chunk_sizes(&b""[..], 4).await.unwrap();
        assert!(sizes.is_empty(), "у пустого источника нет ни одного куска, а нулевой кусок не считается");
    }
}
