// Ещё несколько простых исправлений по Clippy, чтобы ты увидел его пользу.
// TODO: Исправь все линты Clippy.

#[allow(unused_variables, unused_assignments)]
fn main() {
    let my_option: Option<&str> = None;
    // Считай, что значение `my_option` тебе неизвестно.
    // Если там `Some`, мы хотим напечатать его значение.
    if my_option.is_none() {
        println!("{}", my_option.unwrap());
    }

    #[rustfmt::skip]
    let my_arr = &[
        -1, -2, -3
        -4, -5, -6
    ];
    println!("Мой массив! Вот он: {my_arr:?}");

    let mut my_vec = vec![1, 2, 3, 4, 5];
    my_vec.resize(0, 5);
    println!("Этот Vec пуст, видишь? {my_vec:?}");

    let mut value_a = 45;
    let mut value_b = 66;
    // Поменяем эти два значения местами!
    value_a = value_b;
    value_b = value_a;
    println!("значение a: {value_a}; значение b: {value_b}");
}
