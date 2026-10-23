use std::env;
use std::process::ExitCode;
use std::time::Duration;

use social_cli::{
    ADMIN_ADDR_ENV, CA_ENV, DEFAULT_ADMIN_ADDR, DEFAULT_HTTP_ADDR, Dialer, HTTP_ADDR_ENV,
    READY_LIMIT, await_ready, run_with,
};
use tokio::io::{BufReader, stdin, stdout};
use tokio::runtime::Builder;

fn setting(key: &str, default: &str) -> String {
    env::var(key)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let mut addr = setting(HTTP_ADDR_ENV, DEFAULT_HTTP_ADDR);
    let mut admin = Some(setting(ADMIN_ADDR_ENV, DEFAULT_ADMIN_ADDR));
    let mut ca = env::var(CA_ENV).ok().filter(|path| !path.is_empty());
    while let Some(arg) = args.next() {
        if arg == "--no-wait" {
            admin = None;
            continue;
        }
        match (arg.as_str(), args.next()) {
            ("--addr", Some(value)) => addr = value,
            ("--admin", Some(value)) => admin = Some(value),
            ("--ca", Some(value)) => ca = Some(value),
            _ => {
                eprintln!(
                    "social-cli [--addr шлюз] [--admin admin-порт шлюза] [--no-wait] [--ca файл CA], по умолчанию {DEFAULT_HTTP_ADDR} и {DEFAULT_ADMIN_ADDR} или {HTTP_ADDR_ENV}, {ADMIN_ADDR_ENV} и {CA_ENV}; вход и регистрация идут через тот же шлюз"
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
    let finished = runtime.block_on(async {
        if let Some(admin) = &admin {
            await_ready(admin, READY_LIMIT, &mut stdout()).await?;
        }
        run_with(&addr, dialer, BufReader::new(stdin()), stdout()).await
    });
    runtime.shutdown_timeout(Duration::from_millis(100));
    match finished {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("social-cli: {error}");
            ExitCode::FAILURE
        }
    }
}
