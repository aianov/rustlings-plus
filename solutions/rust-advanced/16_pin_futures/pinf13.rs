use std::cell::{Cell, RefCell};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};

pub struct Seats {
    free: Cell<u32>,
}

impl Seats {
    pub fn new(free: u32) -> Rc<Seats> {
        Rc::new(Seats { free: Cell::new(free) })
    }

    pub fn free(&self) -> u32 {
        self.free.get()
    }
}

pub type Log = Rc<RefCell<Vec<String>>>;

pub struct Payment {
    left: u32,
    approve: bool,
}

impl Payment {
    pub fn new(left: u32, approve: bool) -> Payment {
        Payment { left, approve }
    }
}

impl Future for Payment {
    type Output = bool;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<bool> {
        if self.left == 0 {
            Poll::Ready(self.approve)
        } else {
            self.left -= 1;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

struct Hold<'a> {
    seats: &'a Seats,
    keep: bool,
}

impl Drop for Hold<'_> {
    fn drop(&mut self) {
        if !self.keep {
            self.seats.free.set(self.seats.free.get() + 1);
        }
    }
}

pub async fn book(seats: Rc<Seats>, payment: Payment, log: Log) -> bool {
    if seats.free.get() == 0 {
        return false;
    }
    seats.free.set(seats.free.get() - 1);
    let mut hold = Hold { seats: &seats, keep: false };
    let paid = payment.await;
    if paid {
        hold.keep = true;
        log.borrow_mut().push("paid".to_string());
    }
    paid
}

fn main() {
    let seats = Seats::new(1);
    let _booking = book(seats, Payment::new(1, true), Log::default());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::task::Waker;

    fn poll_boxed(fut: &mut Pin<Box<impl Future<Output = bool>>>) -> Poll<bool> {
        fut.as_mut().poll(&mut Context::from_waker(Waker::noop()))
    }

    fn finish(fut: impl Future<Output = bool>) -> bool {
        let mut fut = Box::pin(fut);
        for _ in 0..20 {
            if let Poll::Ready(value) = poll_boxed(&mut fut) {
                return value;
            }
        }
        panic!("бронь не завершилась");
    }

    #[test]
    fn a_paid_seat_stays_taken() {
        let seats = Seats::new(2);
        let log = Log::default();
        assert!(finish(book(seats.clone(), Payment::new(2, true), log.clone())));
        assert_eq!(seats.free(), 1);
        assert_eq!(*log.borrow(), vec!["paid".to_string()]);
    }

    #[test]
    fn a_declined_payment_returns_the_seat() {
        let seats = Seats::new(2);
        let log = Log::default();
        assert!(!finish(book(seats.clone(), Payment::new(2, false), log.clone())));
        assert_eq!(seats.free(), 2);
        assert!(log.borrow().is_empty());
    }

    #[test]
    fn a_cancelled_booking_returns_the_seat() {
        let seats = Seats::new(2);
        let log = Log::default();
        let mut booking = Box::pin(book(seats.clone(), Payment::new(5, true), log.clone()));
        assert_eq!(poll_boxed(&mut booking), Poll::Pending);
        assert_eq!(seats.free(), 1, "пока платёж ждём, место держится за нами");
        drop(booking);
        assert_eq!(seats.free(), 2, "бронь отменили на середине: место обязано вернуться");
        assert!(log.borrow().is_empty());
    }

    #[test]
    fn the_seat_is_held_while_the_payment_is_pending() {
        let seats = Seats::new(1);
        let mut first = Box::pin(book(seats.clone(), Payment::new(3, true), Log::default()));
        assert_eq!(poll_boxed(&mut first), Poll::Pending);
        let mut second = Box::pin(book(seats.clone(), Payment::new(0, true), Log::default()));
        assert_eq!(poll_boxed(&mut second), Poll::Ready(false), "единственное место занято первым, второму нечего занимать");
        assert_eq!(seats.free(), 0);
    }

    #[test]
    fn a_future_that_was_never_polled_holds_nothing() {
        let seats = Seats::new(2);
        let booking = book(seats.clone(), Payment::new(1, true), Log::default());
        assert_eq!(seats.free(), 2, "async fn ленива: пока не опрошена, место не занято");
        drop(booking);
        assert_eq!(seats.free(), 2, "и возвращать нечего");
    }

    #[test]
    fn no_seats_means_no_underflow() {
        let seats = Seats::new(0);
        assert!(!finish(book(seats.clone(), Payment::new(1, true), Log::default())));
        assert_eq!(seats.free(), 0);
    }
}
