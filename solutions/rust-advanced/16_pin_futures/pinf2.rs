use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

pub struct Collect {
    source: VecDeque<u32>,
    need: usize,
    taken: usize,
    sum: u32,
}

pub struct Calm {
    readings: VecDeque<bool>,
    quiet: u32,
    streak: u32,
    polls: u32,
}

impl Collect {
    pub fn new(source: Vec<u32>, need: usize) -> Collect {
        Collect { source: source.into(), need, taken: 0, sum: 0 }
    }
}

impl Future for Collect {
    type Output = u32;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<u32> {
        let this = &mut *self;
        if this.taken < this.need {
            if let Some(number) = this.source.pop_front() {
                this.sum += number;
                this.taken += 1;
            }
        }
        if this.taken == this.need {
            Poll::Ready(this.sum)
        } else {
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

impl Calm {
    pub fn new(readings: Vec<bool>, quiet: u32) -> Calm {
        Calm { readings: readings.into(), quiet, streak: 0, polls: 0 }
    }
}

impl Future for Calm {
    type Output = u32;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<u32> {
        let this = &mut *self;
        this.polls += 1;
        match this.readings.pop_front() {
            Some(true) => this.streak = 0,
            Some(false) => this.streak += 1,
            None => {}
        }
        if this.streak >= this.quiet {
            Poll::Ready(this.polls)
        } else {
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

fn main() {
    let mut cx = Context::from_waker(std::task::Waker::noop());
    let mut collect = Collect::new(vec![1, 2, 3], 2);
    println!("{:?}", Pin::new(&mut collect).poll(&mut cx));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::task::Waker;

    fn poll<F: Future + Unpin>(fut: &mut F) -> Poll<F::Output> {
        Pin::new(fut).poll(&mut Context::from_waker(Waker::noop()))
    }

    #[test]
    fn collect_takes_one_number_per_poll() {
        let mut fut = Collect::new(vec![5, 6, 7, 8], 3);
        assert_eq!(poll(&mut fut), Poll::Pending, "после первого числа ещё рано");
        assert_eq!(poll(&mut fut), Poll::Pending, "после второго тоже");
        assert_eq!(poll(&mut fut), Poll::Ready(18), "на третьем опросе набрано 5 + 6 + 7");
    }

    #[test]
    fn collect_edge_cases() {
        let mut nothing = Collect::new(vec![9], 0);
        assert_eq!(poll(&mut nothing), Poll::Ready(0), "ноль чисел набирать не надо");
        let mut short = Collect::new(vec![1, 2], 3);
        for _ in 0..10 {
            assert_eq!(poll(&mut short), Poll::Pending, "чисел не хватает: ответ только Pending");
        }
    }

    #[test]
    fn calm_counts_the_polls_it_needed() {
        let mut fut = Calm::new(vec![false, true, false, false, false, true], 3);
        for _ in 0..4 {
            assert_eq!(poll(&mut fut), Poll::Pending);
        }
        assert_eq!(poll(&mut fut), Poll::Ready(5), "шум на втором опросе обнулил серию, тишина набралась на пятом");
    }

    #[test]
    fn calm_noise_resets_the_streak() {
        let mut fut = Calm::new(vec![false, false, true, false, false, true, false, false], 3);
        for round in 0..8 {
            assert_eq!(poll(&mut fut), Poll::Pending, "на опросе {} серия из трёх подряд ещё не набралась", round + 1);
        }
    }

    #[test]
    fn calm_ready_at_once_for_zero_quiet() {
        let mut fut = Calm::new(vec![true], 0);
        assert_eq!(poll(&mut fut), Poll::Ready(1));
    }
}
