use std::env;
use std::io::{self, BufRead};

use social_core::{MemNetwork, SharedNetwork, execute_line};

const VERSION: &str = env!("REPL_VERSION");

fn main() -> io::Result<()> {
    if env::args().skip(1).any(|arg| arg == "--version") {
        println!("{VERSION}");
        return Ok(());
    }
    let network = SharedNetwork::new(MemNetwork::new())?;
    for line in io::stdin().lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        match execute_line(&line, &network) {
            Ok(reply) => println!("{reply}"),
            Err(error) => println!("ошибка: {error}"),
        }
    }
    Ok(())
}
