use std::fmt::Debug;

pub trait Mailer {
    fn send(&self, text: &str);
}

use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub struct MockMailer {
    sent: Arc<Mutex<Vec<String>>>,
}

impl MockMailer {
    pub fn sent(&self) -> Vec<String> {
        self.sent.lock().expect("журнал отравлен").clone()
    }
}

impl Mailer for MockMailer {
    fn send(&self, text: &str) {
        self.sent.lock().expect("журнал отравлен").push(text.to_string());
    }
}

pub struct Reports<M: Mailer> {
    mailer: M,
}

impl<M: Mailer> Reports<M> {
    pub fn new(mailer: M) -> Self {
        Reports { mailer }
    }

    pub fn publish<T: Debug>(&self, title: &str, rows: &[T]) {
        self.mailer.send(&format!("{title}: {} строк", rows.len()));
    }
}

fn main() {
    let reports = Reports::new(MockMailer::default());
    reports.publish("итог", &[1, 2, 3]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publish_sends_one_mail() {
        let mailer = MockMailer::default();
        let reports = Reports::new(mailer.clone());
        reports.publish("итог", &[1, 2, 3]);
        assert_eq!(mailer.sent(), vec!["итог: 3 строк".to_string()], "письмо должно быть видно в журнале оригинала");
    }

    #[test]
    fn two_services_share_one_journal() {
        let mailer = MockMailer::default();
        let (a, b) = (Reports::new(mailer.clone()), Reports::new(mailer.clone()));
        a.publish("a", &[1]);
        b.publish("b", &[1, 2]);
        assert_eq!(mailer.sent().len(), 2, "оба сервиса пишут в один журнал");
    }

    #[test]
    fn fresh_mock_starts_empty() {
        let (one, two) = (MockMailer::default(), MockMailer::default());
        Reports::new(one.clone()).publish("x", &[0]);
        assert!(two.sent().is_empty(), "независимо созданные моки не должны делить журнал");
    }

        const SRC: &str = include_str!("tst8.rs");

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


    #[test]
    fn mock_can_move_to_another_thread() {
        assert!(
            compiles_with_lints("fn need<T: Send + Sync + 'static>() {} fn probe() { need::<MockMailer>(); }", &[]),
            "мок должен быть Send + Sync: сервис с ним может жить в другом потоке"
        );
    }
}
