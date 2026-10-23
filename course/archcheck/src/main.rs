use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut positional = Vec::new();
    let mut locale = String::from("en");
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--locale" {
            match args.next() {
                Some(l) => locale = l,
                None => return usage(),
            }
        } else {
            positional.push(a);
        }
    }
    let [project, rules] = positional.as_slice() else {
        return usage();
    };
    let text = match std::fs::read_to_string(rules) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("archcheck: cannot read {rules}: {e}");
            return ExitCode::from(2);
        }
    };
    match archcheck::check(Path::new(project), &text, &locale) {
        Ok(violations) if violations.is_empty() => ExitCode::SUCCESS,
        Ok(violations) => {
            for v in &violations {
                match &v.path {
                    Some(p) => println!("[{}] {}: {}", v.rule, p, v.message),
                    None => println!("[{}] {}", v.rule, v.message),
                }
            }
            ExitCode::from(1)
        }
        Err(e) => {
            eprintln!("archcheck: {e}");
            ExitCode::from(2)
        }
    }
}

fn usage() -> ExitCode {
    eprintln!("usage: archcheck <project_dir> <arch.toml> [--locale ru]");
    ExitCode::from(2)
}
