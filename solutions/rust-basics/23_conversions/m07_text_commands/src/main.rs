use std::io::{self, BufRead};

use backend::{Command, MemNetwork, SharedNetwork};

fn main() -> io::Result<()> {
    let network = SharedNetwork::new(MemNetwork::new())?;
    for line in io::stdin().lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        match line.parse::<Command>().and_then(|command| command.run(&network)) {
            Ok(reply) => println!("{reply}"),
            Err(error) => println!("ошибка: {error}"),
        }
    }
    Ok(())
}
