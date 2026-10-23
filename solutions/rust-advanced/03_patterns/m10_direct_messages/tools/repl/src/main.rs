use std::env;
use std::io::{self, BufRead};

use repl::Terminal;

const VERSION: &str = env!("REPL_VERSION");

fn main() -> io::Result<()> {
    if env::args().skip(1).any(|arg| arg == "--version") {
        println!("{VERSION}");
        return Ok(());
    }
    let mut terminal = Terminal::new()?;
    for line in io::stdin().lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        match terminal.handle(&line) {
            Ok(reply) => println!("{reply}"),
            Err(error) => println!("ошибка: {error}"),
        }
    }
    Ok(())
}
