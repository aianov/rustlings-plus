use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

pub struct Delay {
    left: u32,
    label: &'static str,
    done: bool,
}

impl Delay {
    pub fn new(left: u32, label: &'static str) -> Delay {
        Delay { left, label, done: false }
    }
}

impl Future for Delay {
    type Output = &'static str;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<&'static str> {
        assert!(!self.done, "Delay `{}` опрошен после Ready", self.label);
        if self.left == 0 {
            self.done = true;
            Poll::Ready(self.label)
        } else {
            self.left -= 1;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

enum Stage {
    Fetch(Delay),
    Render(Delay),
    Done,
}

pub struct Pipeline {
    stage: Stage,
    render_len: u32,
    parts: Vec<&'static str>,
}

impl Pipeline {
    pub fn new(fetch_len: u32, render_len: u32) -> Pipeline {
        Pipeline { stage: Stage::Fetch(Delay::new(fetch_len, "fetched")), render_len, parts: Vec::new() }
    }
}

impl Future for Pipeline {
    type Output = String;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<String> {
        let this = &mut *self;
        loop {
            match &mut this.stage {
                Stage::Fetch(delay) => match Pin::new(delay).poll(cx) {
                    Poll::Ready(part) => {
                        this.parts.push(part);
                        this.stage = Stage::Render(Delay::new(this.render_len, "rendered"));
                    }
                    Poll::Pending => return Poll::Pending,
                },
                Stage::Render(delay) => match Pin::new(delay).poll(cx) {
                    Poll::Ready(part) => {
                        this.parts.push(part);
                        this.stage = Stage::Done;
                    }
                    Poll::Pending => return Poll::Pending,
                },
                Stage::Done => return Poll::Ready(this.parts.join(" -> ")),
            }
        }
    }
}

fn main() {
    let _pipeline = Pipeline::new(1, 1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::pin::pin;
    use std::task::Waker;

    fn run(fetch: u32, render: u32) -> (String, u32) {
        let mut fut = pin!(Pipeline::new(fetch, render));
        let mut cx = Context::from_waker(Waker::noop());
        for n in 1..=50 {
            if let Poll::Ready(text) = fut.as_mut().poll(&mut cx) {
                return (text, n);
            }
        }
        panic!("конвейер не закончил за 50 опросов");
    }

    #[test]
    fn both_stages_run_in_order() {
        assert_eq!(run(2, 1), ("fetched -> rendered".to_string(), 4), "2 Pending на загрузке, 1 на вёрстке и ответ на четвёртом опросе");
    }

    #[test]
    fn instant_stages_finish_in_one_poll() {
        assert_eq!(run(0, 0), ("fetched -> rendered".to_string(), 1));
    }

    #[test]
    fn a_slow_render_does_not_repeat_the_fetch() {
        assert_eq!(run(1, 4), ("fetched -> rendered".to_string(), 6));
        assert_eq!(run(0, 3), ("fetched -> rendered".to_string(), 4));
    }
}
