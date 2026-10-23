// Нужно определить корзину фруктов в виде хеш-таблицы. Ключ
// это название фрукта, а значение это количество фруктов
// этого вида в корзине. Нужно положить в корзину минимум 3 разных
// вида фруктов (например, яблоко, банан, манго), а общее количество
// всех фруктов должно быть не меньше 5.

use std::collections::HashMap;

fn fruit_basket() -> HashMap<String, u32> {
    // Объявляем хеш-таблицу.
    let mut basket = HashMap::new();

    // Два банана тебе уже выдали :)
    basket.insert(String::from("banana"), 2);

    // Кладём в корзину ещё фруктов.
    basket.insert(String::from("apple"), 3);
    basket.insert(String::from("mango"), 1);

    basket
}

fn main() {
    // Здесь можно по желанию поэкспериментировать.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_least_three_types_of_fruits() {
        let basket = fruit_basket();
        assert!(basket.len() >= 3);
    }

    #[test]
    fn at_least_five_fruits() {
        let basket = fruit_basket();
        assert!(basket.values().sum::<u32>() >= 5);
    }
}
