use std::io::Read;

fn main() -> std::io::Result<()> {
    let mut text = String::new();
    std::io::stdin().read_to_string(&mut text)?;
    println!("lines: {}", word_count::stats::analyze(&text).lines);
    Ok(())
}
