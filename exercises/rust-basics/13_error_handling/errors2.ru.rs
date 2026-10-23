// Допустим, мы пишем игру, где предметы покупаются за жетоны. Каждый предмет
// стоит 5 жетонов, а при любой покупке взимается комиссия в 1 жетон. Игрок
// вводит, сколько предметов хочет купить, а функция `total_cost` считает
// общую стоимость. Количество вводит игрок, поэтому мы получаем его
// строкой. Он мог напечатать что угодно,
// не только цифры!
//
// Сейчас функция вообще не обрабатывает ошибку. Мы хотим вот чего: если
// вызвать `total_cost` для строки, которая не является числом, функция
// вернёт `ParseIntError`. В этом случае мы хотим сразу вернуть эту ошибку
// из нашей функции и не пытаться умножать и
// складывать.
//
// Есть минимум два способа сделать это, и оба верны. Но один из них гораздо
// короче!

use std::num::ParseIntError;

fn total_cost(item_quantity: &str) -> Result<i32, ParseIntError> {
    let processing_fee = 1;
    let cost_per_item = 5;

    // TODO: Обработай случай ошибки, как описано выше.
    let qty = item_quantity.parse::<i32>();

    Ok(qty * cost_per_item + processing_fee)
}

fn main() {
    // Здесь можно поэкспериментировать.
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::IntErrorKind;

    #[test]
    fn item_quantity_is_a_valid_number() {
        assert_eq!(total_cost("34"), Ok(171));
    }

    #[test]
    fn item_quantity_is_an_invalid_number() {
        assert_eq!(
            total_cost("beep boop").unwrap_err().kind(),
            &IntErrorKind::InvalidDigit,
        );
    }
}
