use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

pub struct Greeting {
    name: Option<String>,
}

impl Greeting {
    pub fn new(name: &str) -> Greeting {
        Greeting { name: Some(name.to_string()) }
    }
}

impl Future for Greeting {
    type Output = String;

    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<String> {
        let name = self.name.take().expect("Greeting опрошен после Ready");
        Poll::Ready(format!("привет, {name}!"))
    }
}

pub fn poll_by_hand<F: Future + Unpin>(fut: &mut F) -> Poll<F::Output> {
    let mut cx = Context::from_waker(Waker::noop());
    Pin::new(fut).poll(&mut cx)
}

pub fn drive<F: Future + Unpin>(mut fut: F, max_polls: usize) -> Option<F::Output> {
    for _ in 0..max_polls {
        if let Poll::Ready(value) = poll_by_hand(&mut fut) {
            return Some(value);
        }
    }
    None
}

fn main() {
    let mut greeting = Greeting::new("ann");
    println!("{:?}", poll_by_hand(&mut greeting));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::panic::{catch_unwind, AssertUnwindSafe};

    struct Slow {
        left: u32,
        polls: u32,
        done: bool,
    }

    impl Slow {
        fn new(left: u32) -> Slow {
            Slow { left, polls: 0, done: false }
        }
    }

    impl Future for Slow {
        type Output = u32;

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<u32> {
            assert!(!self.done, "Slow опрошен после Ready: опросчик не должен звать poll дальше");
            self.polls += 1;
            if self.left == 0 {
                self.done = true;
                Poll::Ready(self.polls)
            } else {
                self.left -= 1;
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }
    }

    #[test]
    fn the_greeting_is_ready_on_the_first_poll() {
        let mut greeting = Greeting::new("ann");
        assert_eq!(poll_by_hand(&mut greeting), Poll::Ready("привет, ann!".to_string()));
    }

    #[test]
    fn the_greeting_is_given_only_once() {
        let mut greeting = Greeting::new("bob");
        let _ = poll_by_hand(&mut greeting);
        let second = catch_unwind(AssertUnwindSafe(|| poll_by_hand(&mut greeting)));
        assert!(second.is_err(), "второй опрос после Ready должен паниковать, а не отдавать приветствие снова");
    }

    #[test]
    fn poll_by_hand_is_exactly_one_poll() {
        let mut slow = Slow::new(2);
        assert_eq!(poll_by_hand(&mut slow), Poll::Pending);
        assert_eq!(poll_by_hand(&mut slow), Poll::Pending);
        assert_eq!(poll_by_hand(&mut slow), Poll::Ready(3));
        assert_eq!(slow.polls, 3, "каждый вызов опросчика это ровно один poll");
    }

    #[test]
    fn drive_stops_at_the_first_ready() {
        let mut slow = Slow::new(3);
        assert_eq!(drive(&mut slow, 10), Some(4));
        assert_eq!(slow.polls, 4, "после Ready опрашивать нельзя");
    }

    #[test]
    fn drive_respects_the_limit() {
        let mut slow = Slow::new(3);
        assert_eq!(drive(&mut slow, 3), None);
        assert_eq!(slow.polls, 3, "лимит это число опросов, не больше и не меньше");
        let mut never = Slow::new(0);
        assert_eq!(drive(&mut never, 0), None);
        assert_eq!(never.polls, 0);
    }
}
