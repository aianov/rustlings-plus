use std::env;
use std::io::{self, BufRead};
use std::sync::Arc;

use repl::Terminal;
use social_core::{LogNotifier, Notifiers};

const VERSION: &str = env!("REPL_VERSION");

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--version") {
        println!("{VERSION}");
        return Ok(());
    }
    let mut notifiers = Notifiers::new();
    if args.iter().any(|arg| arg == "--log") {
        notifiers.add(Arc::new(LogNotifier::new(io::stderr())));
    }
    let mut terminal = Terminal::with_notifiers(notifiers)?;
    for line in io::stdin().lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let response = terminal.handle(&line);
        if response.is_ok() {
            println!("{}", response.body);
        } else {
            println!("ошибка: {}", response.body);
        }
    }
    Ok(())
}
