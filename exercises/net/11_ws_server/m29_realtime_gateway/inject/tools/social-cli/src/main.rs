use std::env;
use std::process::ExitCode;
use std::time::Duration;

use social_cli::{DEFAULT_HTTP_ADDR, HTTP_ADDR_ENV, run};
use tokio::io::{BufReader, stdin, stdout};
use tokio::runtime::Builder;

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let mut addr = env::var(HTTP_ADDR_ENV).unwrap_or_else(|_| DEFAULT_HTTP_ADDR.to_string());
    while let Some(arg) = args.next() {
        match (arg.as_str(), args.next()) {
            ("--addr", Some(value)) => addr = value,
            _ => {
                eprintln!(
                    "social-cli [--addr адрес], по умолчанию {DEFAULT_HTTP_ADDR} или {HTTP_ADDR_ENV}"
                );
                return ExitCode::FAILURE;
            }
        }
    }
    let runtime = match Builder::new_current_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("social-cli: рантайм не запустился: {error}");
            return ExitCode::FAILURE;
        }
    };
    let finished = runtime.block_on(run(&addr, BufReader::new(stdin()), stdout()));
    runtime.shutdown_timeout(Duration::from_millis(100));
    match finished {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("social-cli: {error}");
            ExitCode::FAILURE
        }
    }
}
