use std::io;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader, BufWriter};

#[derive(Debug, Clone, PartialEq)]
struct Event {
    id: u64,
    kind: String,
    text: String,
}

fn invalid_data(why: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, why.to_string())
}

struct Journal<W> {
    out: BufWriter<W>,
    flush_every: usize,
    unflushed: usize,
}

impl<W: AsyncWrite + Unpin> Journal<W> {
    fn new(sink: W, flush_every: usize) -> Journal<W> {
        Journal { out: BufWriter::new(sink), flush_every, unflushed: 0 }
    }

    async fn append(&mut self, event: &Event) -> io::Result<()> {
        if event.kind.contains(['\t', '\n']) || event.text.contains('\n') {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "в событии символ, который ломает формат журнала"));
        }
        let line = format!("{}\t{}\t{}\n", event.id, event.kind, event.text);
        self.out.write_all(line.as_bytes()).await?;
        self.unflushed += 1;
        if self.unflushed >= self.flush_every {
            self.out.flush().await?;
            self.unflushed = 0;
        }
        Ok(())
    }

    async fn close(mut self) -> io::Result<()> {
        self.out.flush().await?;
        self.out.shutdown().await
    }
}

fn parse(line: &[u8]) -> io::Result<Event> {
    let line = std::str::from_utf8(line).map_err(|_| invalid_data("строка журнала не UTF-8"))?;
    let mut parts = line.splitn(3, '\t');
    let (Some(id), Some(kind), Some(text)) = (parts.next(), parts.next(), parts.next()) else {
        return Err(invalid_data("в строке журнала меньше трёх полей"));
    };
    let id = id.parse().map_err(|_| invalid_data("номер события не число"))?;
    Ok(Event { id, kind: kind.to_string(), text: text.to_string() })
}

async fn replay<R: AsyncRead + Unpin>(source: R, max_line: usize) -> io::Result<Vec<Event>> {
    let mut reader = BufReader::new(source);
    let mut events = Vec::new();
    let mut raw = Vec::new();
    loop {
        raw.clear();
        let read = (&mut reader).take(max_line as u64 + 1).read_until(b'\n', &mut raw).await?;
        if read == 0 {
            return Ok(events);
        }
        if raw.last() != Some(&b'\n') {
            if raw.len() > max_line {
                return Err(invalid_data("строка журнала длиннее лимита"));
            }
            return Ok(events);
        }
        raw.pop();
        events.push(parse(&raw)?);
    }
}

#[tokio::main]
async fn main() {
    let mut bytes = Vec::new();
    {
        let mut journal = Journal::new(&mut bytes, 2);
        journal.append(&Event { id: 1, kind: "post".into(), text: "hello".into() }).await.unwrap();
        journal.close().await.unwrap();
    }
    println!("{:?}", replay(&bytes[..], 1024).await.unwrap());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::pin::Pin;
    use std::sync::{Arc, Mutex};
    use std::task::{Context, Poll};
    use std::time::Duration;
    use tokio::io::{duplex, DuplexStream};
    use tokio::time::sleep;

    #[derive(Default)]
    struct State {
        data: Vec<u8>,
        shut: bool,
    }

    #[derive(Clone, Default)]
    struct Probe(Arc<Mutex<State>>);

    impl Probe {
        fn text(&self) -> String {
            String::from_utf8(self.0.lock().unwrap().data.clone()).unwrap()
        }
    }

    impl AsyncWrite for Probe {
        fn poll_write(self: Pin<&mut Self>, _: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
            self.0.lock().unwrap().data.extend_from_slice(buf);
            Poll::Ready(Ok(buf.len()))
        }

        fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
            self.0.lock().unwrap().shut = true;
            Poll::Ready(Ok(()))
        }
    }

    fn event(id: u64, kind: &str, text: &str) -> Event {
        Event { id, kind: kind.to_string(), text: text.to_string() }
    }

    fn trickle(bytes: Vec<u8>) -> DuplexStream {
        let (mut tx, rx) = duplex(16);
        tokio::spawn(async move {
            for piece in bytes.chunks(3) {
                tx.write_all(piece).await.unwrap();
                sleep(Duration::from_millis(1)).await;
            }
        });
        rx
    }

    #[tokio::test]
    async fn flush_happens_every_n_appends() {
        let probe = Probe::default();
        let mut journal = Journal::new(probe.clone(), 2);
        for id in 1..=3 {
            journal.append(&event(id, "post", "x")).await.unwrap();
        }
        assert_eq!(
            probe.text(),
            "1\tpost\tx\n2\tpost\tx\n",
            "после трёх записей при flush_every = 2 в приёмнике должны лежать ровно две: третья ждёт сброса"
        );
        journal.append(&event(4, "post", "x")).await.unwrap();
        assert_eq!(probe.text().lines().count(), 4, "четвёртая запись сбрасывает буфер, видны все четыре");
    }

    #[tokio::test]
    async fn close_flushes_the_rest_and_shuts_down() {
        let probe = Probe::default();
        let mut journal = Journal::new(probe.clone(), 100);
        journal.append(&event(1, "like", "a")).await.unwrap();
        assert_eq!(probe.text(), "", "до порога сброса данные остаются в буфере");
        journal.close().await.unwrap();
        assert_eq!(probe.text(), "1\tlike\ta\n", "close должен сбросить остаток");
        assert!(probe.0.lock().unwrap().shut, "close должен закрыть запись у приёмника");
    }

    #[tokio::test]
    async fn broken_events_are_rejected_without_a_trace() {
        let probe = Probe::default();
        let mut journal = Journal::new(probe.clone(), 1);
        for bad in [event(1, "po\tst", "x"), event(2, "post", "two\nlines"), event(3, "a\nb", "x")] {
            let err = journal.append(&bad).await.unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::InvalidInput, "событие {} ломает формат: ждём InvalidInput", bad.id);
        }
        journal.append(&event(4, "post", "tab\tin text is fine")).await.unwrap();
        journal.close().await.unwrap();
        assert_eq!(probe.text(), "4\tpost\ttab\tin text is fine\n", "отклонённые события не должны оставить в журнале ни байта");
    }

    #[tokio::test(start_paused = true)]
    async fn journal_survives_a_roundtrip_through_a_trickling_stream() {
        let mut bytes = Vec::new();
        let mut journal = Journal::new(&mut bytes, 3);
        let events = vec![event(1, "post", "привет, мир"), event(2, "follow", ""), event(3, "post", "a\tb"), event(4, "like", "7")];
        for e in &events {
            journal.append(e).await.unwrap();
        }
        journal.close().await.unwrap();
        let got = replay(trickle(bytes), 1024).await.unwrap();
        assert_eq!(got, events, "после проигрывания должны вернуться те же события в том же порядке");
    }

    #[tokio::test]
    async fn torn_tail_is_dropped() {
        let got = replay(&b"1\tpost\tok\n2\tpost\thalf writ"[..], 1024).await.unwrap();
        assert_eq!(got, vec![event(1, "post", "ok")], "недописанная последняя строка не должна попасть в восстановленное состояние");
    }

    struct Metered<R> {
        inner: R,
        taken: Arc<Mutex<usize>>,
    }

    impl<R: AsyncRead + Unpin> AsyncRead for Metered<R> {
        fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut tokio::io::ReadBuf<'_>) -> Poll<io::Result<()>> {
            let before = buf.filled().len();
            let poll = Pin::new(&mut self.inner).poll_read(cx, buf);
            *self.taken.lock().unwrap() += buf.filled().len() - before;
            poll
        }
    }

    #[tokio::test]
    async fn long_line_is_refused_without_reading_it_all() {
        let mut data = b"1\tpost\tok\n".to_vec();
        data.extend_from_slice(b"2\tpost\t");
        let taken = Arc::new(Mutex::new(0));
        let source = Metered { inner: (&data[..]).chain(tokio::io::repeat(b'x').take(1 << 22)), taken: taken.clone() };
        let err = replay(source, 64).await.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData, "строка длиннее лимита: ждём InvalidData");
        let taken = *taken.lock().unwrap();
        assert!(taken < (1 << 16), "из источника вычитано {taken} байт: слишком длинную строку нельзя читать целиком");
    }

    #[tokio::test]
    async fn line_of_exactly_max_is_fine() {
        let line = "9\tpost\tabc";
        let data = format!("{line}\n");
        let got = replay(data.as_bytes(), line.len()).await.unwrap();
        assert_eq!(got, vec![event(9, "post", "abc")], "строка ровно в лимит допустима");
        let err = replay(data.as_bytes(), line.len() - 1).await.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData, "на байт длиннее лимита уже нельзя");
    }

    #[tokio::test]
    async fn garbage_complete_line_is_an_error() {
        for bad in ["nonsense\n", "x\tpost\ttext\n", "5\tpost\n"] {
            let err = replay(bad.as_bytes(), 1024).await.unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::InvalidData, "целую, но не разбираемую строку {bad:?} нельзя пропускать молча");
        }
    }
}
