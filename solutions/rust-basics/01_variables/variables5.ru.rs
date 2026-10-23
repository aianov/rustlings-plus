fn main() {
    let number = "T-H-R-E-E";
    println!("Число прописью: {number}");

    // Используем затенение переменной
    // https://doc.rust-lang.org/book/ch03-01-variables-and-mutability.html#shadowing
    let number = 3;
    println!("Число плюс два равно: {}", number + 2);
}
