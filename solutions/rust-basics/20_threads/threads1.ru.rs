// Эта программа запускает несколько потоков, каждый из которых работает не
// меньше 250 мс и возвращает, сколько времени он занял. Программа должна
// дождаться завершения всех запущенных потоков и собрать их возвращаемые
// значения в вектор.

use std::{
    thread,
    time::{Duration, Instant},
};

fn main() {
    let mut handles = Vec::new();
    for i in 0..10 {
        let handle = thread::spawn(move || {
            let start = Instant::now();
            thread::sleep(Duration::from_millis(250));
            println!("Поток {i} завершён");
            start.elapsed().as_millis()
        });
        handles.push(handle);
    }

    let mut results = Vec::new();
    for handle in handles {
        // Собираем результаты всех потоков в вектор `results`.
        results.push(handle.join().unwrap());
    }

    if results.len() != 10 {
        panic!("Ой! Какой-то поток ещё не закончил!");
    }

    println!();
    for (i, result) in results.into_iter().enumerate() {
        println!("Поток {i} занял {result} мс");
    }
}
