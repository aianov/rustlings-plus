# Потоки

В большинстве современных операционных систем код запущенной программы выполняется в
процессе, а операционная система управляет сразу многими процессами. Внутри программы тоже
могут быть независимые части, работающие одновременно. Средства, которые их запускают,
называются потоками.

## Кратко

`thread::spawn` запускает замыкание в новом потоке и возвращает `JoinHandle`. Вызов `join()`
ждёт завершения потока и отдаёт его результат. Без `join` главный поток может закончиться
раньше и убить остальные.

```rust
use std::{sync::{mpsc, Arc, Mutex}, thread};

let counter = Arc::new(Mutex::new(0));
let mut handles = Vec::new();
for _ in 0..4 {
    let counter = Arc::clone(&counter);
    handles.push(thread::spawn(move || {
        *counter.lock().unwrap() += 1;
    }));
}
for h in handles { h.join().unwrap(); }

let (tx, rx) = mpsc::channel();
thread::spawn(move || tx.send(42).unwrap());
println!("{}", rx.recv().unwrap());
```

Общее изменяемое состояние делается через `Arc<Mutex<T>>`: `Arc` делит владение, `Mutex`
пускает по одному потоку за раз. Альтернатива это каналы `mpsc`: потоки не делят память, а
передают друг другу сообщения. Отправителя можно клонировать, получатель один.

## Дополнительные материалы

- [Пример «Обедающие философы»](https://doc.rust-lang.org/1.4.0/book/dining-philosophers.html)
- [Потоки для одновременного выполнения кода](https://doc.rust-lang.ru/book/ch16-01-threads.html)
- [Передача данных между потоками через сообщения](https://doc.rust-lang.ru/book/ch16-02-message-passing.html)
