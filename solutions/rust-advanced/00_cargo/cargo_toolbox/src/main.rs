use std::io::Read;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version") {
        println!("{}", textstat::VERSION);
        return ExitCode::SUCCESS;
    }
    let mut text = String::new();
    if std::io::stdin().read_to_string(&mut text).is_err() {
        eprintln!("не удалось прочитать stdin");
        return ExitCode::FAILURE;
    }
    if let Some(i) = args.iter().position(|a| a == "--top") {
        let n = args.get(i + 1).and_then(|n| n.parse().ok()).unwrap_or(10);
        for (word, count) in textstat::top_words(&text, n) {
            println!("{word} {count}");
        }
        return ExitCode::SUCCESS;
    }
    let report = textstat::analyze(&text);
    if args.iter().any(|a| a == "--json") {
        return print_json(&report);
    }
    println!("{}", textstat::render_text(&report));
    ExitCode::SUCCESS
}

#[cfg(feature = "json")]
fn print_json(report: &textstat::Report) -> ExitCode {
    println!("{}", textstat::render_json(report));
    ExitCode::SUCCESS
}

#[cfg(not(feature = "json"))]
fn print_json(_report: &textstat::Report) -> ExitCode {
    eprintln!("textstat собран без feature json");
    ExitCode::from(2)
}
