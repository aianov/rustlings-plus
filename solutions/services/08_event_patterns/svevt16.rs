fn main() {}

pub trait Step {
    fn name(&self) -> &str;
    fn run(&self) -> Result<(), String>;
    fn compensate(&self) -> Result<(), String>;
}

#[derive(Debug, PartialEq)]
pub enum Outcome {
    Completed,
    RolledBack { failed: String, error: String },
    Stuck { failed: String, error: String, uncompensated: Vec<String> },
}

pub fn run_saga(steps: &[Box<dyn Step>]) -> Outcome {
    let mut done: Vec<&dyn Step> = Vec::new();
    for step in steps {
        match step.run() {
            Ok(()) => done.push(step.as_ref()),
            Err(error) => {
                let mut uncompensated = Vec::new();
                for finished in done.iter().rev() {
                    let mut compensated = false;
                    for _ in 0..3 {
                        if finished.compensate().is_ok() {
                            compensated = true;
                            break;
                        }
                    }
                    if !compensated {
                        uncompensated.push(finished.name().to_string());
                    }
                }
                let failed = step.name().to_string();
                return if uncompensated.is_empty() { Outcome::RolledBack { failed, error } } else { Outcome::Stuck { failed, error, uncompensated } };
            }
        }
    }
    Outcome::Completed
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::{Arc, Mutex};

    struct Probe {
        name: &'static str,
        run_error: Option<&'static str>,
        compensation_failures: AtomicU32,
        log: Arc<Mutex<Vec<String>>>,
    }

    impl Step for Probe {
        fn name(&self) -> &str {
            self.name
        }

        fn run(&self) -> Result<(), String> {
            self.log.lock().unwrap().push(format!("run {}", self.name));
            self.run_error.map_or(Ok(()), |e| Err(e.to_string()))
        }

        fn compensate(&self) -> Result<(), String> {
            self.log.lock().unwrap().push(format!("undo {}", self.name));
            if self.compensation_failures.load(Ordering::SeqCst) > 0 {
                self.compensation_failures.fetch_sub(1, Ordering::SeqCst);
                return Err("сервис не отвечает".to_string());
            }
            Ok(())
        }
    }

    type Spec = (&'static str, Option<&'static str>, u32);
    type Log = Arc<Mutex<Vec<String>>>;

    fn saga(spec: &[Spec]) -> (Vec<Box<dyn Step>>, Log) {
        let log = Arc::new(Mutex::new(Vec::new()));
        let steps = spec
            .iter()
            .map(|(name, run_error, failures)| {
                Box::new(Probe { name, run_error: *run_error, compensation_failures: AtomicU32::new(*failures), log: log.clone() }) as Box<dyn Step>
            })
            .collect();
        (steps, log)
    }

    fn lines(log: &Log) -> Vec<String> {
        log.lock().unwrap().clone()
    }

    #[test]
    fn when_everything_succeeds_nothing_is_undone() {
        let (steps, log) = saga(&[("freeze", None, 0), ("hide_posts", None, 0), ("erase", None, 0)]);
        assert_eq!(run_saga(&steps), Outcome::Completed, "сага завершилась");
        assert_eq!(lines(&log), vec!["run freeze", "run hide_posts", "run erase"], "шаги по порядку, откатов нет");
    }

    #[test]
    fn a_failure_undoes_the_finished_steps_in_reverse_and_skips_the_rest() {
        let (steps, log) = saga(&[("freeze", None, 0), ("hide_posts", None, 0), ("revoke", Some("auth-service лёг"), 0), ("erase", None, 0)]);
        assert_eq!(run_saga(&steps), Outcome::RolledBack { failed: "revoke".into(), error: "auth-service лёг".into() }, "итог: откат, названы шаг и причина");
        assert_eq!(
            lines(&log),
            vec!["run freeze", "run hide_posts", "run revoke", "undo hide_posts", "undo freeze"],
            "откатываются только выполненные шаги, в обратном порядке; упавший не откатывается, четвёртый не запускается"
        );
    }

    #[test]
    fn a_failure_of_the_first_step_has_nothing_to_undo() {
        let (steps, log) = saga(&[("freeze", Some("нет связи"), 0), ("erase", None, 0)]);
        assert!(matches!(run_saga(&steps), Outcome::RolledBack { .. }), "откатывать нечего, но итог всё равно откат");
        assert_eq!(lines(&log), vec!["run freeze"], "ни откатов, ни следующих шагов");
    }

    #[test]
    fn a_flaky_compensation_is_retried() {
        let (steps, log) = saga(&[("freeze", None, 2), ("erase", Some("отказ"), 0)]);
        assert!(matches!(run_saga(&steps), Outcome::RolledBack { .. }), "третья попытка удалась: откат полный");
        assert_eq!(lines(&log).iter().filter(|l| *l == "undo freeze").count(), 3, "две неудачи и успех: три попытки");
    }

    #[test]
    fn a_stuck_compensation_does_not_stop_the_others_and_is_reported() {
        let (steps, log) = saga(&[("freeze", None, 0), ("hide_posts", None, 99), ("revoke", None, 0), ("erase", Some("отказ"), 0)]);
        let outcome = run_saga(&steps);
        assert_eq!(
            outcome,
            Outcome::Stuck { failed: "erase".into(), error: "отказ".into(), uncompensated: vec!["hide_posts".into()] },
            "не откатился один шаг: о нём надо сообщить"
        );
        let undo: Vec<String> = lines(&log).into_iter().filter(|l| l.starts_with("undo")).collect();
        assert_eq!(
            undo,
            vec!["undo revoke", "undo hide_posts", "undo hide_posts", "undo hide_posts", "undo freeze"],
            "ровно три попытки, и после застрявшей компенсации остальные всё равно выполнены"
        );
    }
}
