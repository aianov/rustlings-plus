use std::io;

use contracts::{Entry, write_line};
use tokio::io::{AsyncWrite, AsyncWriteExt, BufWriter};

pub struct JournalWriter<W> {
    out: BufWriter<W>,
    line: Vec<u8>,
    flush_every: usize,
    unflushed: usize,
}

impl<W: AsyncWrite + Unpin> JournalWriter<W> {
    pub fn new(sink: W, flush_every: usize) -> JournalWriter<W> {
        JournalWriter {
            out: BufWriter::new(sink),
            line: Vec::new(),
            flush_every: flush_every.max(1),
            unflushed: 0,
        }
    }

    pub fn unflushed(&self) -> usize {
        self.unflushed
    }

    pub async fn append(&mut self, entry: &Entry) -> io::Result<()> {
        self.line.clear();
        write_line(&mut self.line, entry)?;
        self.out.write_all(&self.line).await?;
        self.unflushed += 1;
        if self.unflushed >= self.flush_every {
            self.out.flush().await?;
            self.unflushed = 0;
        }
        Ok(())
    }

    pub async fn close(mut self) -> io::Result<()> {
        self.out.flush().await?;
        self.out.shutdown().await
    }
}
