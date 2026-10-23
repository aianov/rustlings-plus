// Обработчик «считай обращения» хранит счётчик в `Rc<RefCell<u32>>`: просто и быстро. Реестр обработчиков
// общий для потоков, и тип `Handler` требует `Send + Sync`. Теперь тест пробует вызвать счётчик из
// другого потока, и компилятор сообщает, что `Rc` нельзя отправлять между потоками.
// TODO: Почини `counting_handler`: тип возвращаемого счётчика и его начинка должны подходить для потоков
// (в std::sync есть потокобезопасные аналоги и для `Rc`, и для `RefCell`, а тесты читают счётчик через них).
// Сам обработчик по-прежнему возвращает значение счётчика после увеличения.

use std::cell::RefCell;
use std::rc::Rc;

pub type Handler = Box<dyn Fn() -> u32 + Send + Sync + 'static>;

pub fn counting_handler() -> (Handler, Rc<RefCell<u32>>) {
    let counter = Rc::new(RefCell::new(0));
    let inner = Rc::clone(&counter);
    let handler: Handler = Box::new(move || {
        *inner.borrow_mut() += 1;
        *inner.borrow()
    });
    (handler, counter)
}

fn main() {
    let (h, c) = counting_handler();
    println!("{} {:?}", h(), c);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn counts_in_one_thread() {
        let (handler, counter) = counting_handler();
        assert_eq!(handler(), 1);
        assert_eq!(handler(), 2);
        assert_eq!(*counter.lock().expect("счётчик отравлен"), 2, "счётчик снаружи видит то же значение");
    }

    #[test]
    fn counts_across_threads() {
        let (handler, counter) = counting_handler();
        let handler = Arc::new(handler);
        let workers: Vec<_> = (0..8)
            .map(|_| {
                let handler = Arc::clone(&handler);
                std::thread::spawn(move || {
                    for _ in 0..100 {
                        handler();
                    }
                })
            })
            .collect();
        for w in workers {
            w.join().expect("поток упал");
        }
        assert_eq!(*counter.lock().expect("счётчик отравлен"), 800, "ни одно обращение не должно потеряться");
    }
}
