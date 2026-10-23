use std::sync::Arc;
use std::thread::{self, JoinHandle};

pub trait Notifier: Send + Sync {
    fn notify(&self, text: &str) -> String;
}

pub struct Console;

impl Notifier for Console {
    fn notify(&self, text: &str) -> String {
        format!("console: {text}")
    }
}

pub fn spawn_notify(notifier: Arc<dyn Notifier>, text: String) -> JoinHandle<String> {
    thread::spawn(move || notifier.notify(&text))
}

fn main() {
    let h = spawn_notify(Arc::new(Console), "hi".to_string());
    println!("{}", h.join().expect("поток упал"));
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = include_str!("dyn15.rs");

    #[test]
    fn notifier_goes_to_a_thread() {
        let n: Arc<dyn Notifier> = Arc::new(Console);
        let handles: Vec<_> = (0..3).map(|i| spawn_notify(Arc::clone(&n), format!("m{i}"))).collect();
        let got: Vec<String> = handles.into_iter().map(|h| h.join().expect("поток упал")).collect();
        assert_eq!(got, vec!["console: m0", "console: m1", "console: m2"]);
    }

    fn compiles_with(extra: &str) -> bool {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!("rustlings_dyn15_{}_{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        std::fs::create_dir_all(&dir).expect("не удалось создать папку");
        let file = dir.join("probe.rs");
        std::fs::write(&file, format!("{SRC}\n{extra}\n")).expect("не удалось записать файл");
        let ok = std::process::Command::new("rustc")
            .args(["--edition", "2024", "--crate-type", "lib", "--emit=metadata", "-A", "warnings", "--out-dir"])
            .arg(&dir)
            .arg(&file)
            .output()
            .expect("не удалось запустить rustc")
            .status
            .success();
        let _ = std::fs::remove_dir_all(&dir);
        ok
    }

    #[test]
    fn thread_safe_types_may_implement_it() {
        assert!(compiles_with("pub struct Quiet; impl Notifier for Quiet { fn notify(&self, t: &str) -> String { t.to_string() } }"));
    }

    #[test]
    fn non_thread_safe_types_may_not() {
        assert!(
            !compiles_with("pub struct Local(std::rc::Rc<u8>); impl Notifier for Local { fn notify(&self, t: &str) -> String { t.to_string() } }"),
            "тип с Rc внутри смог реализовать Notifier: трейт должен требовать Send и Sync от самих реализаций"
        );
    }
}
