// Мэри покупает яблоки. Цена яблока считается так:
// - Яблоко стоит 2 растбакса.
// - Но если Мэри покупает больше 40 яблок, цена каждого яблока во
// всём заказе снижается до одного растбакса!

fn calculate_price_of_apples(n_apples: u64) -> u64 {
    if n_apples > 40 {
        n_apples
    } else {
        2 * n_apples
    }
}

fn main() {
    // Здесь можно по желанию поэкспериментировать.
}

// Тесты не меняй!
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_test() {
        assert_eq!(calculate_price_of_apples(35), 70);
        assert_eq!(calculate_price_of_apples(40), 80);
        assert_eq!(calculate_price_of_apples(41), 41);
        assert_eq!(calculate_price_of_apples(65), 65);
    }
}
