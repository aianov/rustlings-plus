use std::io::{self, BufRead, Write};
use std::sync::Arc;

use contracts::write_line;
use social_core::{
    MemNetwork, MemPostStore, MemUserStore, Notifiers, PushQueue, SharedNetwork, serve_line,
};

use crate::error::{Error, Result};
use crate::terminal::Terminal;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Text,
    Json,
}

pub fn run(
    mode: Mode,
    mut notifiers: Notifiers,
    input: impl BufRead,
    mut output: impl Write,
) -> Result<()> {
    match mode {
        Mode::Text => {
            let mut terminal = Terminal::with_notifiers(notifiers)?;
            pump(input, |line| {
                let response = terminal.handle(line);
                if response.is_ok() {
                    writeln!(output, "{}", response.body)
                } else {
                    writeln!(output, "ошибка: {}", response.body)
                }
            })
        }
        Mode::Json => {
            let pushes = Arc::new(PushQueue::new());
            notifiers.add(pushes.clone());
            let net: SharedNetwork<MemUserStore, MemPostStore> =
                SharedNetwork::with_notifiers(MemNetwork::new(), notifiers)?;
            pump(input, |line| {
                write_line(&mut output, &serve_line(&net, line))?;
                for push in pushes.drain() {
                    write_line(&mut output, &push)?;
                }
                Ok(())
            })
        }
    }
}

fn pump(input: impl BufRead, mut handle: impl FnMut(&str) -> io::Result<()>) -> Result<()> {
    for (index, line) in input.lines().enumerate() {
        let line = line.map_err(|source| Error::Read {
            line: index + 1,
            source,
        })?;
        if line.trim().is_empty() {
            continue;
        }
        handle(&line).map_err(Error::Write)?;
    }
    Ok(())
}
