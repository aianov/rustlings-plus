// `RawBox<T>` владеет значением в куче через сырой указатель, как `Box`. Сырые указатели не `Send` и не `Sync`,
// поэтому автор добавил два `unsafe impl`, чтобы тип можно было передавать в потоки. Но обещание без условий:
// теперь через `RawBox<Rc<_>>` поток получает `Rc`, а `Rc` считает ссылки без синхронизации. Компилятор молчит,
// потому что `unsafe impl` это твоё слово против его проверки.
// TODO: Поправь оба `unsafe impl` так, чтобы `RawBox<T>` был `Send` и `Sync` ровно тогда, когда это верно для `T`.
use std::marker::PhantomData;

pub struct RawBox<T> {
    ptr: *mut T,
    owns: PhantomData<T>,
}

unsafe impl<T> Send for RawBox<T> {}
unsafe impl<T> Sync for RawBox<T> {}

impl<T> RawBox<T> {
    pub fn new(value: T) -> Self {
        RawBox { ptr: Box::into_raw(Box::new(value)), owns: PhantomData }
    }

    pub fn get(&self) -> &T {
        // SAFETY: ptr получен из Box::into_raw и живёт до Drop.
        unsafe { &*self.ptr }
    }

    pub fn set(&mut self, value: T) {
        // SAFETY: &mut self даёт единственный доступ, старое значение освобождается присваиванием.
        unsafe { *self.ptr = value };
    }
}

impl<T> Drop for RawBox<T> {
    fn drop(&mut self) {
        // SAFETY: ptr получен из Box::into_raw и больше нигде не освобождается.
        drop(unsafe { Box::from_raw(self.ptr) });
    }
}

fn main() {
    let mut b = RawBox::new(1);
    b.set(2);
    println!("{}", b.get());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::thread;

    const SRC: &str = include_str!("uns9.rs");

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

    fn holds(marker: &str, ty: &str) -> bool {
        compiles(&format!("fn need<T: {marker}>() {{}}\nfn probe() {{ need::<{ty}>(); }}"))
    }

    struct Counted(Arc<AtomicUsize>);

    impl Drop for Counted {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn the_box_reads_writes_and_drops_once() {
        let drops = Arc::new(AtomicUsize::new(0));
        {
            let mut b = RawBox::new(Counted(Arc::clone(&drops)));
            assert_eq!(drops.load(Ordering::SeqCst), 0);
            b.set(Counted(Arc::clone(&drops)));
            assert_eq!(drops.load(Ordering::SeqCst), 1, "старое значение освобождается при set");
            assert_eq!(b.get().0.load(Ordering::SeqCst), 1);
        }
        assert_eq!(drops.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn a_box_of_strings_travels_to_another_thread() {
        let text = thread::spawn({
            let b = RawBox::new(String::from("привет"));
            move || b.get().to_uppercase()
        })
        .join()
        .unwrap();
        assert_eq!(text, "ПРИВЕТ");
    }

    #[test]
    fn plain_data_is_send_and_sync() {
        assert!(holds("Send", "RawBox<i32>"));
        assert!(holds("Sync", "RawBox<i32>"));
        assert!(holds("Send", "RawBox<Vec<String>>"));
    }

    #[test]
    fn rc_inside_is_neither_send_nor_sync() {
        assert!(!holds("Send", "RawBox<std::rc::Rc<i32>>"), "RawBox<Rc<_>> нельзя отправлять в поток: Rc считает ссылки без синхронизации");
        assert!(!holds("Sync", "RawBox<std::rc::Rc<i32>>"));
    }

    #[test]
    fn cell_inside_can_move_but_not_be_shared() {
        assert!(holds("Send", "RawBox<std::cell::Cell<i32>>"), "Cell можно переместить в другой поток");
        assert!(!holds("Sync", "RawBox<std::cell::Cell<i32>>"), "&Cell из двух потоков менять нельзя, значит RawBox<Cell<_>> не Sync");
    }
}
