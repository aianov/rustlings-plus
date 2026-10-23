mod bench;
mod certs;
mod check;
mod migrate;
mod root;
mod service;

use std::env;
use std::process::ExitCode;

const USAGE: &str = "использование: cargo xtask <check | bench | migrate | new-service <имя> | certs [--force | --keep]>";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match args.as_slice() {
        ["check"] => root::find().and_then(|root| check::run(&root)),
        ["bench"] => root::find().and_then(|root| bench::run(&root)),
        ["migrate"] => migrate::run(),
        ["new-service", name] => root::find().and_then(|root| service::create(&root, name)),
        ["certs"] => root::find().and_then(|root| certs::run(&root, None)),
        ["certs", flag] => root::find().and_then(|root| certs::run(&root, Some(flag))),
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("xtask: {message}");
            ExitCode::FAILURE
        }
    }
}
