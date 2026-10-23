fn main() {
    let mut res = 42;
    let option = Some(12);
    // Используем `if-let` вместо итерации.
    if let Some(x) = option {
        res += x;
    }

    println!("{res}");
}
