use std::io::Write;
use std::process::ExitCode;

use loadgen::{
    Closed, Mode, Open, Stop, compare, parse_args, ramp, run_closed, run_open, stack_target,
};

fn say(line: &str) {
    let _ = writeln!(std::io::stdout(), "{line}");
}

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let gateway = std::env::var("GATEWAY_HTTP_ADDR").ok();
    let options = match parse_args(&args, gateway) {
        Ok(options) => options,
        Err(error) => {
            let _ = writeln!(std::io::stderr(), "{error}");
            return ExitCode::FAILURE;
        }
    };
    let hit = match stack_target(&options.gateway, options.users, options.scenario.clone()).await {
        Ok(hit) => hit,
        Err(error) => {
            let _ = writeln!(
                std::io::stderr(),
                "шлюз {} не готов к нагрузке: {error}",
                options.gateway
            );
            return ExitCode::FAILURE;
        }
    };
    let report = match options.mode {
        Mode::Closed => {
            let model = Closed {
                users: options.users,
                duration: options.duration,
            };
            run_closed(model, hit).await.report("закрытая модель")
        }
        Mode::Open => {
            let model = Open {
                rate: options.rate,
                duration: options.duration,
                workers: options.users,
            };
            run_open(model, hit).await.report("открытая модель")
        }
        Mode::Ramp => {
            let duration = options.duration;
            let workers = options.users;
            let outcome = ramp(options.ramp, |rate| {
                let hit = hit.clone();
                async move {
                    let model = Open {
                        rate,
                        duration,
                        workers,
                    };
                    let report = run_open(model, hit)
                        .await
                        .report(&format!("ступень {rate:.0}/с"));
                    say(&report.render());
                    report
                }
            })
            .await;
            match &outcome.stop {
                Stop::Latency { rate, p99_ms } => say(&format!(
                    "на {rate:.0}/с p99 {p99_ms:.1} мс вышел за предел"
                )),
                Stop::Errors { rate, error_rate } => say(&format!(
                    "на {rate:.0}/с доля ошибок {:.2}%",
                    error_rate * 100.0
                )),
                Stop::OutOfSteps => say("ступени кончились раньше предела"),
            }
            match outcome.best {
                Some((rate, report)) => {
                    say(&format!("держит {rate:.0} в секунду"));
                    report
                }
                None => {
                    say("не держит даже первой ступени");
                    return ExitCode::FAILURE;
                }
            }
        }
    };
    say(&report.render());
    if let Some(path) = &options.compare {
        if let Ok(text) = std::fs::read_to_string(path) {
            if let Ok(before) = loadgen::Report::from_json(&text) {
                say(&compare(&before, &report));
            }
        }
    }
    if let Some(path) = &options.save {
        if let Err(error) = std::fs::write(path, report.to_json()) {
            let _ = writeln!(std::io::stderr(), "отчёт не сохранён в {path}: {error}");
        }
    }
    ExitCode::SUCCESS
}
