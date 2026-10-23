mod error;
mod file;
mod replay;
mod writer;

use tokio::io::AsyncWrite;

pub use error::JournalError;
pub use file::load;
pub use replay::{Replayed, replay};
pub use writer::JournalWriter;

pub type Sink = Box<dyn AsyncWrite + Send + Unpin>;

pub struct Journal {
    sink: Sink,
}

impl Journal {
    pub fn new(sink: impl AsyncWrite + Send + Unpin + 'static) -> Journal {
        Journal {
            sink: Box::new(sink),
        }
    }

    pub fn into_sink(self) -> Sink {
        self.sink
    }
}
