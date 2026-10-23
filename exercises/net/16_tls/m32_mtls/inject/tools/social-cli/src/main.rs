use std::env;
use std::process::ExitCode;
use std::time::Duration;

use social_cli::{
    AUTH_ADDR_ENV, CA_ENV, DEFAULT_AUTH_ADDR, DEFAULT_HTTP_ADDR, Dialer, HTTP_ADDR_ENV, run_with,
};
use tokio::io::{BufReader, stdin, stdout};
use tokio::runtime::Builder;

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let mut addr = env::var(HTTP_ADDR_ENV).unwrap_or_else(|_| DEFAULT_HTTP_ADDR.to_string());
    let mut auth = env::var(AUTH_ADDR_ENV).unwrap_or_else(|_| DEFAULT_AUTH_ADDR.to_string());
    let mut ca = env::var(CA_ENV).ok().filter(|path| !path.is_empty());
    while let Some(arg) = args.next() {
        match (arg.as_str(), args.next()) {
            ("--addr", Some(value)) => addr = value,
            ("--auth", Some(value)) => auth = value,
            ("--ca", Some(value)) => ca = Some(value),
            _ => {
                eprintln!(
                    "social-cli [--addr шлюз] [--auth вход] [--ca файл CA], по умолчанию {DEFAULT_HTTP_ADDR} и {DEFAULT_AUTH_ADDR} или {HTTP_ADDR_ENV}, {AUTH_ADDR_ENV} и {CA_ENV}"
                );
                return ExitCode::FAILURE;
            }
        }
    }
    let dialer = match ca {
        None => Dialer::plain(),
        Some(path) => match std::fs::read(&path).and_then(|pem| Dialer::trusting(&pem)) {
            Ok(dialer) => dialer,
            Err(error) => {
                eprintln!("social-cli: CA из {path} не читается: {error}");
                return ExitCode::FAILURE;
            }
        },
    };
    let runtime = match Builder::new_current_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("social-cli: рантайм не запустился: {error}");
            return ExitCode::FAILURE;
        }
    };
    let finished = runtime.block_on(run_with(
        &addr,
        &auth,
        dialer,
        BufReader::new(stdin()),
        stdout(),
    ));
    runtime.shutdown_timeout(Duration::from_millis(100));
    match finished {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("social-cli: {error}");
            ExitCode::FAILURE
        }
    }
}
