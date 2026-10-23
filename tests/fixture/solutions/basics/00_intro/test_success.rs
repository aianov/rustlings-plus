// Everything already works here.
fn main() {
    // Print something.
    println!("Output from `main` function");
}

#[cfg(test)]
mod tests {
    #[test]
    fn pass() {}
}
