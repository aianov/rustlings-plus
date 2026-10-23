// В этом магазине распродажа: если цена чётная, скидка 10
// растбаксов, а если нечётная, то 3 растбакса.
// Сами тела функций сейчас не важны, нас пока интересуют только
// сигнатуры.

fn is_even(num: i64) -> bool {
    num % 2 == 0
}

// TODO: Исправь сигнатуру функции.
fn sale_price(price: i64) -> {
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
