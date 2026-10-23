pub fn repeat<F: FnMut()>(times: u32, mut f: F) {
    for _ in 0..times {
        f();
    }
}

pub fn once<T, F: FnOnce() -> T>(f: F) -> T {
    f()
}

pub fn probe<F: Fn() -> u32>(f: &F) -> (u32, u32) {
    (f(), f())
}

fn main() {
    let mut n = 0;
    repeat(3, || n += 1);
    println!("{n}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn repeat_accepts_a_counter() {
        let mut n = 0;
        repeat(4, || n += 1);
        assert_eq!(n, 4, "замыкание-счётчик должно вызваться четыре раза");
    }

    #[test]
    fn repeat_accepts_plain_closures_too() {
        let hits = Cell::new(0);
        repeat(2, || hits.set(hits.get() + 1));
        assert_eq!(hits.get(), 2);
    }

    #[test]
    fn once_accepts_a_closure_that_gives_away_its_data() {
        let name = String::from("ann");
        let got = once(move || name);
        assert_eq!(got, "ann", "замыкание отдаёт то, чем владеет, значит, годится только для одного вызова");
    }

    #[test]
    fn once_accepts_a_consuming_closure() {
        let items = vec![1, 2, 3];
        let total: i32 = once(move || items.into_iter().sum());
        assert_eq!(total, 6);
    }

    #[test]
    fn probe_calls_through_a_shared_reference() {
        let base = Cell::new(10);
        let next = || {
            base.set(base.get() + 1);
            base.get()
        };
        assert_eq!(probe(&next), (11, 12), "через общую ссылку можно звать только Fn; изменение идёт через Cell");
    }
}
