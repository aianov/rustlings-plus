fn is_even(num: i64) -> bool {
    num % 2 == 0
}

// Тип возвращаемого значения нужно указывать всегда.
fn sale_price(price: i64) -> i64 {
    if is_even(price) {
        price - 10
    } else {
        price - 3
    }
}

fn main() {
    let original_price = 51;
    println!("Твоя цена со скидкой: {}", sale_price(original_price));
}
