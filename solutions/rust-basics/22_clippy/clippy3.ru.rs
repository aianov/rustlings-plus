use std::mem;

#[allow(unused_variables, unused_assignments)]
fn main() {
    let my_option: Option<&str> = None;
    // `unwrap` для `Option` после проверки на `None` вызовет панику.
    // Используй вместо этого `if-let`.
    if let Some(value) = my_option {
        println!("{value}");
    }

    // Не хватало запятой.
    #[rustfmt::skip]
    let my_arr = &[
        -1, -2, -3,
        -4, -5, -6,
    ];
    println!("Мой массив! Вот он: {my_arr:?}");

    let mut my_vec = vec![1, 2, 3, 4, 5];
    // `resize` изменяет вектор, а не возвращает новый.
    // `resize(0, …)` очищает вектор, поэтому лучше использовать `clear`.
    my_vec.clear();
    println!("Этот Vec пуст, видишь? {my_vec:?}");

    let mut value_a = 45;
    let mut value_b = 66;
    // Используй `mem::swap`, чтобы правильно поменять два значения местами.
    mem::swap(&mut value_a, &mut value_b);
    println!("значение a: {value_a}; значение b: {value_b}");
}
