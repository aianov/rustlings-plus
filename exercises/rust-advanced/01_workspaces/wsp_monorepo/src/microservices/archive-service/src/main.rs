mod handler;

use std::io::BufRead;

fn main() {
    let stdin = std::io::stdin();
    let lines: Vec<String> = stdin.lock().lines().map_while(Result::ok).collect();
    println!("{}", handler::archive(&lines));
}
