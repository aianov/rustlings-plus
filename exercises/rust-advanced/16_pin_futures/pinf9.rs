// Future из `async fn` нельзя двигать после первого опроса: внутри автомата может лежать ссылка на
// его же собственную локальную переменную, и при перемещении она осталась бы указывать на старое место.
// Поэтому `poll` принимает не `&mut Self`, а `Pin<&mut Self>`: закреплённую ссылку, из которой
// переложить значение нельзя. Для типов без таких ссылок (`Unpin`) закрепление ничего не стоит,
// для остальных его надо оформить: на стеке макросом `pin!` или в куче через `Box::pin`.
// Три помощника ниже получают как раз такие future (в тестах это `async fn` с ссылкой через `.await`).
// Писать `unsafe` не нужно и нельзя.
// TODO: Допиши `poll_to_end`, `boxed` и `finish_order`.
use std::future::Future;
use std::pin::{pin, Pin};
use std::task::{Context, Poll, Waker};

pub type BoxFut<T> = Pin<Box<dyn Future<Output = T>>>;

/// Опрашивает future не больше `max_polls` раз, готовый результат отдаёт.
pub fn poll_to_end<F: Future>(fut: F, max_polls: usize) -> Option<F::Output> {
    todo!("закрепить future на стеке и опрашивать")
}

/// Прячет любой future в закреплённый ящик, чтобы его можно было хранить рядом с другими.
pub fn boxed<T>(fut: impl Future<Output = T> + 'static) -> BoxFut<T> {
    todo!()
}

/// Опрашивает все future по кругу (каждый раз по одному опросу каждого, в порядке списка) и
/// возвращает результаты в том порядке, в котором future завершались.
pub fn finish_order(mut futs: Vec<BoxFut<&'static str>>) -> Vec<&'static str> {
    todo!("раунды опросов; ограничь число раундов, например сотней")
}

fn main() {
    println!("{:?}", poll_to_end(async { 1 }, 3));
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = include_str!("pinf9.rs");

    fn code_only() -> String {
        SRC.split("#[cfg(test)]")
            .next()
            .unwrap_or("")
            .lines()
            .map(|line| line.split("//").next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\n")
    }

    struct Yield(bool);

    impl Future for Yield {
        type Output = ();

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
            if self.0 {
                Poll::Ready(())
            } else {
                self.0 = true;
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }
    }

    async fn slow(label: &'static str, pauses: u32) -> &'static str {
        let letters = [label];
        let first = &letters[0];
        for _ in 0..pauses {
            Yield(false).await;
        }
        first
    }

    #[test]
    fn async_blocks_with_self_references_run_to_the_end() {
        assert_eq!(poll_to_end(slow("a", 3), 10), Some("a"));
        assert_eq!(poll_to_end(slow("b", 0), 1), Some("b"));
    }

    #[test]
    fn the_poll_limit_is_respected() {
        assert_eq!(poll_to_end(slow("a", 3), 3), None, "четыре опроса нужно, три не хватает");
        assert_eq!(poll_to_end(slow("a", 3), 4), Some("a"));
    }

    #[test]
    fn plain_unpin_futures_work_too() {
        assert_eq!(poll_to_end(std::future::ready(5), 1), Some(5));
    }

    #[test]
    fn futures_finish_in_the_order_of_their_pauses() {
        let futs = vec![boxed(slow("a", 2)), boxed(slow("b", 0)), boxed(slow("c", 1))];
        assert_eq!(finish_order(futs), vec!["b", "c", "a"], "в круге опросов первым заканчивает тот, у кого меньше пауз");
    }

    #[test]
    fn equal_pauses_finish_in_list_order() {
        let futs = vec![boxed(slow("x", 1)), boxed(slow("y", 1)), boxed(slow("z", 1))];
        assert_eq!(finish_order(futs), vec!["x", "y", "z"]);
    }

    #[test]
    fn futures_can_be_moved_around_after_they_started() {
        let mut first = boxed(slow("moved", 2));
        let mut cx = Context::from_waker(Waker::noop());
        assert!(first.as_mut().poll(&mut cx).is_pending());
        let mut shelf = vec![boxed(slow("other", 1))];
        shelf.push(first);
        shelf.reverse();
        assert_eq!(finish_order(shelf), vec!["moved", "other"], "закреплённый future можно перекладывать в ящике, он остаётся на месте в куче");
    }

    #[test]
    fn no_unsafe_is_needed() {
        assert!(!code_only().contains("unsafe"), "закрепить future можно без unsafe: пригодятся `pin!` и `Box::pin`");
    }
}
