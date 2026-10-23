// Эта программа пытается использовать готовую версию функции `total_cost`
// из предыдущего упражнения. Но она не работает! Почему? Что нужно сделать,
// чтобы это исправить?

use std::num::ParseIntError;

// Эту функцию не меняй.
fn total_cost(item_quantity: &str) -> Result<i32, ParseIntError> {
    let processing_fee = 1;
    let cost_per_item = 5;
    let qty = item_quantity.parse::<i32>()?;

    Ok(qty * cost_per_item + processing_fee)
}

fn main() -> Result<(), ParseIntError> {
    //    ^^^^^^^^^^^^^^^^^^^^^^^^^^^^ добавлено
    let mut tokens = 100;
    let pretend_user_input = "8";

    let cost = total_cost(pretend_user_input)?;

    if cost > tokens {
        println!("Вам столько не потянуть!");
    } else {
        tokens -= cost;
        println!("Теперь у вас {tokens} жетонов.");
    }

    // Добавили эту строку, чтобы вернуть вариант `Ok` ожидаемого `Result`.
    Ok(())
}
