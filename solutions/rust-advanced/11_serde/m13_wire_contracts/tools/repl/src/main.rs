use std::env;
use std::error::Error;
use std::io;
use std::process::ExitCode;
use std::sync::Arc;

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
    repl::run(mode, notifiers, io::stdin().lock(), io::stdout().lock())?;
    Ok(())
}
