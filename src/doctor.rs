use anyhow::Result;
use crossterm::{
    QueueableCommand,
    style::{Color, ResetColor, SetForegroundColor},
};
use std::{
    io::{self, Write},
    process::{Command, Stdio},
};

use crate::{
    i18n::{fill, tr},
    services::{SERVICES, address, reachable},
};

const MIN_RUST: (u32, u32) = (1, 88);
const MIN_FREE_GB: u64 = 5;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Level {
    Ok,
    Problem,
    Optional,
}

pub struct Check {
    pub level: Level,
    pub label: String,
    pub advice: Option<String>,
}

fn output_of(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

pub fn parse_version(text: &str) -> Option<(u32, u32, u32)> {
    let word = text
        .split_whitespace()
        .find(|w| w.chars().next().is_some_and(|c| c.is_ascii_digit()) && w.contains('.'))?;
    let mut parts = word
        .split(|c: char| !c.is_ascii_digit())
        .filter(|p| !p.is_empty())
        .map(|p| p.parse::<u32>().ok());
    Some((
        parts.next()??,
        parts.next()??,
        parts.next().flatten().unwrap_or(0),
    ))
}

fn rust_tool(program: &str) -> Check {
    match output_of(program, &["--version"]) {
        None => Check {
            level: Level::Problem,
            label: program.to_owned(),
            advice: Some(format!(
                "{} {}",
                fill(tr().doc_missing, &[&program]),
                tr().doc_rust_fix
            )),
        },
        Some(text) => {
            let version = parse_version(&text);
            let ok = version.is_some_and(|(a, b, _)| (a, b) >= MIN_RUST);
            Check {
                level: if ok { Level::Ok } else { Level::Problem },
                advice: (!ok).then(|| {
                    let shown =
                        version.map_or_else(|| text.clone(), |(a, b, c)| format!("{a}.{b}.{c}"));
                    fill(tr().doc_too_old, &[&program, &shown])
                }),
                label: text,
            }
        }
    }
}

fn simple(
    label: &str,
    program: &str,
    args: &[&str],
    level_if_missing: Level,
    advice: &str,
) -> Check {
    match output_of(program, args) {
        Some(text) => Check {
            level: Level::Ok,
            label: text.lines().next().unwrap_or(label).to_owned(),
            advice: None,
        },
        None => Check {
            level: level_if_missing,
            label: label.to_owned(),
            advice: Some(format!("{} {advice}", fill(tr().doc_missing, &[&label]))),
        },
    }
}

pub fn free_gb(df_output: &str) -> Option<u64> {
    let line = df_output.lines().nth(1)?;
    let available_kb: u64 = line.split_whitespace().nth(3)?.parse().ok()?;
    Some(available_kb / (1024 * 1024))
}

pub fn checks() -> Vec<Check> {
    let t = tr();
    let mut checks = vec![
        rust_tool("rustc"),
        rust_tool("cargo"),
        simple(
            "cargo clippy",
            "cargo",
            &["clippy", "--version"],
            Level::Problem,
            t.doc_clippy_fix,
        ),
    ];
    let docker = simple(
        "docker",
        "docker",
        &["--version"],
        Level::Problem,
        t.doc_docker_fix,
    );
    let has_docker = docker.level == Level::Ok;
    checks.push(docker);
    if has_docker {
        checks.push(simple(
            "docker compose",
            "docker",
            &["compose", "version"],
            Level::Problem,
            t.doc_compose_fix,
        ));
    }
    for service in &SERVICES {
        let Some((host, port)) = address(service.name) else {
            continue;
        };
        let addr = format!("{host}:{port}");
        let up = reachable(&host, port);
        checks.push(Check {
            level: if up { Level::Ok } else { Level::Problem },
            label: format!("{} {addr}", service.name),
            advice: (!up).then(|| fill(t.doc_service_down, &[&service.name, &addr, &service.name])),
        });
    }
    checks.push(simple(
        "protoc",
        "protoc",
        &["--version"],
        Level::Problem,
        t.doc_protoc_fix,
    ));
    checks.push(simple(
        "cargo-make",
        "cargo",
        &["make", "--version"],
        Level::Optional,
        t.doc_make_fix,
    ));
    if let Some(df) = output_of("df", &["-Pk", "."])
        && let Some(gb) = free_gb(&df)
    {
        let ok = gb >= MIN_FREE_GB;
        checks.push(Check {
            level: if ok { Level::Ok } else { Level::Problem },
            label: fill(t.doc_disk, &[&gb]),
            advice: (!ok).then(|| fill(t.doc_disk_low, &[&MIN_FREE_GB])),
        });
    }
    checks
}

pub fn run() -> Result<bool> {
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "{}", tr().doctor_title)?;
    let checks = checks();
    let mut problems = 0;
    for check in &checks {
        let (mark, color) = match check.level {
            Level::Ok => ("[ok]", Color::Green),
            Level::Problem => ("[!!]", Color::Red),
            Level::Optional => ("[ ?]", Color::Yellow),
        };
        if check.level == Level::Problem {
            problems += 1;
        }
        stdout.queue(SetForegroundColor(color))?;
        write!(stdout, "  {mark} ")?;
        stdout.queue(ResetColor)?;
        writeln!(stdout, "{}", check.label)?;
        if let Some(advice) = &check.advice {
            writeln!(stdout, "       {advice}")?;
        }
    }
    writeln!(stdout)?;
    if problems == 0 {
        stdout.queue(SetForegroundColor(Color::Green))?;
        writeln!(stdout, "{}", tr().doctor_all_good)?;
        stdout.queue(ResetColor)?;
    } else {
        writeln!(stdout, "{}", fill(tr().doctor_problems, &[&problems]))?;
    }
    Ok(problems == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(
            parse_version("rustc 1.88.0 (6b00bc388 2025-06-23)"),
            Some((1, 88, 0))
        );
        assert_eq!(
            parse_version("cargo 1.90.1 (873a06493 2025-05-10)"),
            Some((1, 90, 1))
        );
        assert_eq!(parse_version("clippy 0.1.88"), Some((0, 1, 88)));
        assert_eq!(parse_version("no version"), None);
        assert!((1, 87) < MIN_RUST && (1, 88) >= MIN_RUST && (2, 0) >= MIN_RUST);
    }

    #[test]
    fn disk_space() {
        let df = "Filesystem 1024-blocks Used Available Capacity Mounted on\n/dev/disk3s1 971350180 500000000 52428800 60% /\n";
        assert_eq!(free_gb(df), Some(50));
        assert_eq!(free_gb("garbage"), None);
    }

    #[test]
    fn real_checks_run() {
        let checks = checks();
        assert!(checks.iter().any(|c| c.label.starts_with("rustc")));
        assert!(
            checks
                .iter()
                .all(|c| c.level == Level::Ok || c.advice.is_some())
        );
    }
}
