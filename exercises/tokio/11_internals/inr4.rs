// Таймеры рантайма живут не в потоках, а в одном драйвере: куча дедлайнов и по `Waker` на каждый.
// Поток драйвера спит ровно до ближайшего дедлайна, просыпается и будит всех, чей срок вышел.
// Здесь время условное (целые тики). Нужно: регистрация, смена `Waker` при повторном опросе,
// отмена (выброшенный `Sleep` не должен будить и не должен задерживать сон драйвера), ближайший
// живой дедлайн, `advance` с пробуждением по порядку сроков и сам `Sleep` как future.
// TODO: Напиши методы `Driver` и реализации для `Sleep`.
use std::cell::RefCell;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

struct Driver {
    now: u64,
    heap: BinaryHeap<Reverse<(u64, u64)>>,
    live: HashMap<u64, Waker>,
    next_id: u64,
}

struct Sleep {
    driver: Rc<RefCell<Driver>>,
    deadline: u64,
    id: Option<u64>,
}

impl Sleep {
    fn new(driver: &Rc<RefCell<Driver>>, ticks: u64) -> Sleep {
        let deadline = driver.borrow().now + ticks;
        Sleep { driver: driver.clone(), deadline, id: None }
    }
}

impl Driver {
    fn new() -> Driver {
        Driver { now: 0, heap: BinaryHeap::new(), live: HashMap::new(), next_id: 1 }
    }

    fn register(&mut self, deadline: u64, waker: Waker) -> u64 {
        let _ = (deadline, waker);
        todo!()
    }

    fn update(&mut self, id: u64, waker: Waker) {
        let _ = (id, waker);
        todo!()
    }

    fn cancel(&mut self, id: u64) {
        let _ = id;
        todo!()
    }

    fn next_deadline(&mut self) -> Option<u64> {
        todo!()
    }

    fn park_timeout(&mut self) -> Option<u64> {
        todo!()
    }

    fn advance(&mut self, to: u64) -> usize {
        let _ = to;
        todo!()
    }
}

impl Future for Sleep {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let _ = cx;
        todo!()
    }
}

impl Drop for Sleep {
    fn drop(&mut self) {
        todo!()
    }
}

fn main() {
    let driver = Rc::new(RefCell::new(Driver::new()));
    let _sleep = Sleep::new(&driver, 5);
    println!("{:?}", driver.borrow_mut().park_timeout());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::task::Wake;

    struct Logger {
        id: u64,
        log: Arc<Mutex<Vec<u64>>>,
    }

    impl Wake for Logger {
        fn wake(self: Arc<Self>) {
            self.log.lock().unwrap().push(self.id);
        }
    }

    fn logger(id: u64, log: &Arc<Mutex<Vec<u64>>>) -> Waker {
        Waker::from(Arc::new(Logger { id, log: log.clone() }))
    }

    struct Counter(AtomicUsize);

    impl Wake for Counter {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn counting() -> (Waker, Arc<Counter>) {
        let counter = Arc::new(Counter(AtomicUsize::new(0)));
        (Waker::from(counter.clone()), counter)
    }

    fn shared() -> Rc<RefCell<Driver>> {
        Rc::new(RefCell::new(Driver::new()))
    }

    #[test]
    fn wakes_in_deadline_order() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut d = Driver::new();
        d.register(30, logger(3, &log));
        d.register(10, logger(1, &log));
        d.register(20, logger(2, &log));
        assert_eq!(d.advance(25), 2, "к моменту 25 вышли два срока: 10 и 20");
        assert_eq!(*log.lock().unwrap(), vec![1, 2], "будить надо по порядку сроков, а не регистрации");
        assert_eq!(d.next_deadline(), Some(30), "остался один срок");
        assert_eq!(d.advance(30), 1);
        assert_eq!(*log.lock().unwrap(), vec![1, 2, 3]);
    }

    #[test]
    fn equal_deadlines_wake_in_registration_order() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut d = Driver::new();
        d.register(10, logger(1, &log));
        d.register(10, logger(2, &log));
        d.register(10, logger(3, &log));
        d.advance(10);
        assert_eq!(*log.lock().unwrap(), vec![1, 2, 3], "одинаковый срок: кто раньше записался, того раньше будим");
    }

    #[test]
    fn a_cancelled_timer_neither_wakes_nor_delays_the_driver() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut d = Driver::new();
        let first = d.register(10, logger(1, &log));
        d.register(20, logger(2, &log));
        d.cancel(first);
        assert_eq!(d.next_deadline(), Some(20), "отменённый таймер на вершине кучи не должен определять сон драйвера");
        assert_eq!(d.advance(15), 0, "отменённого будить не надо");
        assert!(log.lock().unwrap().is_empty(), "никто не должен проснуться");
        assert_eq!(d.advance(20), 1);
        assert_eq!(*log.lock().unwrap(), vec![2]);
    }

    #[test]
    fn park_timeout_is_the_distance_to_the_nearest_live_deadline() {
        let (waker, _) = counting();
        let mut d = Driver::new();
        assert_eq!(d.park_timeout(), None, "таймеров нет: спать можно сколько угодно");
        d.advance(4);
        d.register(10, waker.clone());
        assert_eq!(d.park_timeout(), Some(6), "сейчас 4, срок 10: спим 6 тиков");
        d.advance(12);
        assert_eq!(d.park_timeout(), None, "срок вышел и разбужен: спать дальше можно без таймера");
    }

    #[test]
    fn a_sleep_future_wakes_at_its_deadline() {
        let driver = shared();
        let (waker, counter) = counting();
        let mut sleep = Sleep::new(&driver, 10);
        assert_eq!(Pin::new(&mut sleep).poll(&mut Context::from_waker(&waker)), Poll::Pending, "срок не вышел");
        driver.borrow_mut().advance(9);
        assert_eq!(counter.0.load(Ordering::SeqCst), 0, "на 9-м тике рано");
        driver.borrow_mut().advance(10);
        assert_eq!(counter.0.load(Ordering::SeqCst), 1, "на 10-м тике ждущего будят");
        assert_eq!(Pin::new(&mut sleep).poll(&mut Context::from_waker(&waker)), Poll::Ready(()), "после срока Sleep готов");
    }

    #[test]
    fn a_dropped_sleep_leaves_nothing_behind() {
        let driver = shared();
        let (waker, counter) = counting();
        let mut sleep = Sleep::new(&driver, 10);
        let _ = Pin::new(&mut sleep).poll(&mut Context::from_waker(&waker));
        drop(sleep);
        assert_eq!(driver.borrow_mut().next_deadline(), None, "выброшенный Sleep снимает свой таймер");
        assert_eq!(driver.borrow_mut().advance(100), 0);
        assert_eq!(counter.0.load(Ordering::SeqCst), 0, "будить выброшенного некому");
    }

    #[test]
    fn a_repolled_sleep_keeps_one_timer_and_wakes_the_latest_waker() {
        let driver = shared();
        let (first, first_count) = counting();
        let (second, second_count) = counting();
        let mut sleep = Sleep::new(&driver, 10);
        let _ = Pin::new(&mut sleep).poll(&mut Context::from_waker(&first));
        let _ = Pin::new(&mut sleep).poll(&mut Context::from_waker(&second));
        assert_eq!(driver.borrow_mut().advance(10), 1, "повторный опрос не должен заводить второй таймер");
        assert_eq!(second_count.0.load(Ordering::SeqCst), 1, "разбудить надо того, кто опрашивал последним");
        assert_eq!(first_count.0.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn a_zero_sleep_is_ready_without_touching_the_driver() {
        let driver = shared();
        let (waker, _) = counting();
        let mut sleep = Sleep::new(&driver, 0);
        assert_eq!(Pin::new(&mut sleep).poll(&mut Context::from_waker(&waker)), Poll::Ready(()), "нулевой сон готов сразу");
        assert_eq!(driver.borrow_mut().next_deadline(), None, "таймер для него регистрировать не нужно");
    }
}
