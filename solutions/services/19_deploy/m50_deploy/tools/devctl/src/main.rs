use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use devctl::{Devctl, social};
use service_runtime::Source;

const USAGE: &str =
    "devctl up | down | restart <сервис> | status   [--bin target/debug] [--state .devctl]";

fn say(line: &str) {
    let _ = writeln!(std::io::stdout(), "{line}");
}

fn complain(line: &str) -> ExitCode {
    let _ = writeln!(std::io::stderr(), "{line}");
    ExitCode::FAILURE
}

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str, default: &str| {
        args.iter()
            .position(|arg| arg == name)
            .and_then(|at| args.get(at + 1))
            .map_or_else(|| PathBuf::from(default), PathBuf::from)
    };
    let bin = flag("--bin", "target/debug");
    let state = flag("--state", ".devctl");
    let source = match Source::load(".env") {
        Ok(source) => source,
        Err(error) => return complain(&format!("окружение не прочиталось: {error}")),
    };
    let plan = social(&source, &bin);
    let devctl = Devctl::new(state);
    match args.first().map(String::as_str) {
        Some("up") => match devctl.up(&plan).await {
            Ok(started) => {
                say(&format!("подняты по порядку: {}", started.join(", ")));
                ExitCode::SUCCESS
            }
            Err(error) => complain(&format!("стек не поднялся, поднятое остановлено: {error}")),
        },
        Some("down") => match devctl.down(&plan).await {
            Ok(stopped) => {
                for (name, how) in stopped {
                    say(&format!("{name}: {how:?}"));
                }
                ExitCode::SUCCESS
            }
            Err(error) => complain(&error),
        },
        Some("restart") => {
            let Some(service) = args.get(1) else {
                return complain(USAGE);
            };
            match devctl.restart(&plan, service).await {
                Ok(done) => {
                    say(&format!("перезапущены по одному: {}", done.join(", ")));
                    ExitCode::SUCCESS
                }
                Err(error) => complain(&error),
            }
        }
        Some("status") => {
            for instance in devctl.status(&plan).await {
                let pid = instance
                    .pid
                    .map_or_else(|| String::from("-"), |pid| pid.to_string());
                say(&format!(
                    "{:<16} pid {pid:<8} {} {}",
                    instance.name,
                    if instance.alive { "жив" } else { "нет" },
                    if instance.ready {
                        "готов"
                    } else {
                        "не готов"
                    }
                ));
            }
            ExitCode::SUCCESS
        }
        _ => complain(USAGE),
    }
}
