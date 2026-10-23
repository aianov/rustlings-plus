fn main() {
    let mut sum = 0i64;
    for i in 0..100_000 {
        sum += tiny_calc::eval(&format!("{i} + 1")).unwrap_or(0);
    }
    assert!(sum > 0);
    println!("ok");
}
