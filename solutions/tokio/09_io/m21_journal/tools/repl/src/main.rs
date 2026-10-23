use std::error::Error;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::{env, io};

use repl::Mode;
use social_core::{LogNotifier, Notifiers, report};

const VERSION: &str = env!("REPL_VERSION");

fn main() -> ExitCode {
    match start() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ошибка: {}", report(&*error));
            ExitCode::FAILURE
        }
    }
}

fn start() -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut mode = Mode::Text;
    let mut notifiers = Notifiers::new();
    for arg in env::args().skip(1) {
        match arg.as_str() {
            "--version" => {
                println!("{VERSION}");
                return Ok(());
            }
            "--log" => notifiers.add(Arc::new(LogNotifier::new(io::stderr()))),
            "--json" => mode = Mode::Json,
            other => return Err(format!("неизвестный флаг {other}").into()),
        }
    }
    let journal = env::var_os(repl::JOURNAL_ENV).map(PathBuf::from);
    let (input, output) = (io::stdin().lock(), io::stdout().lock());
    repl::run_at(mode, notifiers, journal.as_deref(), input, output)?;
    Ok(())
}
