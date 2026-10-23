// Компилятор решает, можно ли отправить значение в другой поток, по двум маркерам: `Send` («можно
// передать целиком») и `Sync` («можно выдать `&T` нескольким потокам», то есть `&T: Send`). Их не пишут
// руками, они выводятся из полей. Ниже таблица фактов о десяти типах, и она заполнена как попало.
// TODO: Для каждого типа поставь верные `send` и `sync`. Тест спрашивает у настоящего rustc.
#![allow(unused_imports)]
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex, MutexGuard};

pub struct Fact {
    pub ty: &'static str,
    pub send: bool,
    pub sync: bool,
}

const fn fact(ty: &'static str, send: bool, sync: bool) -> Fact {
    Fact { ty, send, sync }
}

pub fn facts() -> Vec<Fact> {
    vec![
        fact("Rc<i32>", true, true),
        fact("Arc<i32>", false, false),
        fact("Cell<i32>", false, true),
        fact("Arc<Cell<i32>>", true, true),
        fact("Mutex<Cell<i32>>", false, false),
        fact("Arc<RefCell<Vec<u8>>>", true, true),
        fact("MutexGuard<'static, i32>", true, false),
        fact("Receiver<i32>", false, false),
        fact("*const i32", true, true),
        fact("&'static Cell<i32>", true, false),
    ]
}

fn main() {
    for f in facts() {
        println!("{}: Send={} Sync={}", f.ty, f.send, f.sync);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = include_str!("conc3.rs");

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

    fn really(marker: &str, ty: &str) -> bool {
        compiles(&format!("fn need<T: {marker}>() {{}}\nfn probe() {{ need::<{ty}>(); }}"))
    }

    #[test]
    fn the_table_covers_all_ten_types() {
        assert_eq!(facts().len(), 10);
    }

    #[test]
    fn send_column_matches_the_compiler() {
        for f in facts() {
            assert_eq!(f.send, really("Send", f.ty), "{}: Send указан неверно, rustc считает иначе", f.ty);
        }
    }

    #[test]
    fn sync_column_matches_the_compiler() {
        for f in facts() {
            assert_eq!(f.sync, really("Sync", f.ty), "{}: Sync указан неверно, rustc считает иначе", f.ty);
        }
    }

    #[test]
    fn the_probe_itself_can_tell_yes_from_no() {
        assert!(really("Send", "String"));
        assert!(!really("Send", "Rc<String>"));
    }
}
