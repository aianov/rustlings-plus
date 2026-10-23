use std::cell::UnsafeCell;

pub struct MyCell<T> {
    value: UnsafeCell<T>,
}

impl<T> MyCell<T> {
    pub fn new(value: T) -> Self {
        MyCell { value: UnsafeCell::new(value) }
    }

    pub fn set(&self, value: T) {
        drop(self.replace(value));
    }

    pub fn replace(&self, value: T) -> T {
        // SAFETY: тип не Sync, значит доступ идёт из одного потока, а ссылки на внутреннее значение наружу
        // не выдаются (только копии и перемещения), поэтому эта &mut единственная на время вызова.
        std::mem::replace(unsafe { &mut *self.value.get() }, value)
    }

    pub fn into_inner(self) -> T {
        self.value.into_inner()
    }
}

impl<T: Copy> MyCell<T> {
    pub fn get(&self) -> T {
        // SAFETY: тип не Sync, ссылок на внутреннее значение нет, читаем копию.
        unsafe { *self.value.get() }
    }
}

impl<T: Default> MyCell<T> {
    pub fn take(&self) -> T {
        self.replace(T::default())
    }
}

fn main() {
    let cell = MyCell::new(1);
    cell.set(2);
    println!("{}", cell.get());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;

    const SRC: &str = include_str!("uns6.rs");

    fn compiles_with_lints(extra: &str, deny: &[&str]) -> bool {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!("rustlings_probe_{}_{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        std::fs::create_dir_all(&dir).expect("не удалось создать папку");
        let file = dir.join("probe.rs");
        let code = SRC.split("#[cfg(test)]").next().unwrap_or("");
        std::fs::write(&file, format!("{code}\n{extra}\n")).expect("не удалось записать файл");
        let mut cmd = std::process::Command::new("rustc");
        cmd.args(["--edition", "2024", "--crate-type", "lib", "--emit=metadata", "-A", "warnings"]);
        for lint in deny {
            cmd.args(["-D", lint]);
        }
        let ok = cmd.arg("--out-dir").arg(&dir).arg(&file).output().expect("не удалось запустить rustc").status.success();
        let _ = std::fs::remove_dir_all(&dir);
        ok
    }


    fn compiles(extra: &str) -> bool {
        compiles_with_lints(extra, &[])
    }

    struct Visits {
        hits: MyCell<u32>,
    }

    impl Visits {
        fn bump(&self) -> u32 {
            self.hits.set(self.hits.get() + 1);
            self.hits.get()
        }
    }

    #[test]
    fn a_shared_reference_is_enough_to_change_the_value() {
        let visits = Visits { hits: MyCell::new(0) };
        let (r1, r2) = (&visits, &visits);
        assert_eq!(r1.bump(), 1);
        assert_eq!(r2.bump(), 2);
        assert_eq!(visits.hits.get(), 2);
    }

    #[test]
    fn replace_take_and_into_inner_move_values_out() {
        let cell = MyCell::new(String::from("первое"));
        assert_eq!(cell.replace(String::from("второе")), "первое");
        assert_eq!(cell.take(), "второе");
        assert_eq!(cell.take(), "", "после take остаётся значение по умолчанию");
        cell.set(String::from("третье"));
        assert_eq!(cell.into_inner(), "третье");
    }

    #[test]
    fn set_drops_the_old_value_exactly_once() {
        let token = Rc::new(());
        let cell = MyCell::new(Rc::clone(&token));
        assert_eq!(Rc::strong_count(&token), 2);
        cell.set(Rc::new(()));
        assert_eq!(Rc::strong_count(&token), 1, "старое значение должно быть освобождено при set");
        let inner = MyCell::new(Rc::clone(&token));
        drop(inner);
        assert_eq!(Rc::strong_count(&token), 1, "значение освобождается вместе с ячейкой, один раз");
    }

    #[test]
    fn the_cell_cannot_be_shared_between_threads() {
        assert!(compiles("fn need<T: Send>() {}\nfn probe() { need::<MyCell<i32>>(); }"), "MyCell<i32> можно передавать в другой поток");
        assert!(
            !compiles("fn need<T: Sync>() {}\nfn probe() { need::<MyCell<i32>>(); }"),
            "MyCell не должен быть Sync: из двух потоков через &MyCell менять значение нельзя"
        );
    }

    #[test]
    fn send_follows_the_content() {
        assert!(
            !compiles("fn need<T: Send>() {}\nfn probe() { need::<MyCell<std::rc::Rc<i32>>>(); }"),
            "MyCell<Rc<_>> не должен быть Send"
        );
    }
}
