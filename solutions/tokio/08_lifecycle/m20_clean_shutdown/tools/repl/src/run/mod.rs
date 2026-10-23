use std::io::{self, BufRead, Write};
use std::sync::Arc;

use contracts::write_line;
use social_core::{App, MemNetwork, Notifiers, PushQueue, serve_line};

use crate::error::{Error, Result};
use crate::runtime;
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
            let runtime = runtime::start()?;
            let net: App = {
                let _entered = runtime.enter();
                App::with_notifiers(MemNetwork::new(), notifiers)?
            };
            let pumped = pump(input, |line| {
                let response = runtime.block_on(async {
                    let response = serve_line(&net, line).await;
                    let _ = net.flush().await;
                    response
                });
                write_line(&mut output, &response)?;
                for push in pushes.drain() {
                    write_line(&mut output, &push)?;
                }
                Ok(())
            });
            runtime.block_on(net.close(runtime::CLOSE_DEADLINE));
            pumped
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
