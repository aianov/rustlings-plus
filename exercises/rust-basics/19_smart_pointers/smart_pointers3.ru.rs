// В этом упражнении у нас есть `Vec` из `u32` по имени `numbers` со
// значениями от 0 до 99. Мы хотим использовать этот набор чисел сразу в 8
// разных потоках. Каждый поток посчитает сумму каждого восьмого значения
// начиная со своего смещения.
//
// Первый поток (смещение 0) суммирует 0, 8, 16, …
// Второй поток (смещение 1) суммирует 1, 9, 17, …
// Третий поток (смещение 2) суммирует 2, 10, 18, …
// …
// Восьмой поток (смещение 7) суммирует 7, 15, 23, …
//
// Каждый поток должен владеть указателем с подсчётом ссылок на вектор
// чисел. Но `Rc` не потокобезопасен. Поэтому нужен `Arc`.
//
// Не отвлекайся на то, как потоки запускаются и присоединяются. Это мы
// потренируем позже в упражнениях про потоки.

// Строки ниже не меняй.
#![forbid(unused_imports)]
use std::{sync::Arc, thread};

fn main() {
    let numbers: Vec<_> = (0..100u32).collect();

    // TODO: Определи `shared_numbers` с помощью `Arc`.
    // let shared_numbers = ???;

    let mut join_handles = Vec::new();

    for offset in 0..8 {
        // TODO: Определи `child_numbers` через `shared_numbers`.
        // let child_numbers = ???;

        let handle = thread::spawn(move || {
            let sum: u32 = child_numbers.iter().filter(|&&n| n % 8 == offset).sum();
            println!("Сумма для смещения {offset}: {sum}");
        });

        join_handles.push(handle);
    }

    for handle in join_handles.into_iter() {
        handle.join().unwrap();
    }
}
