fn main() {}

#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    Started(String),
    Done(String),
    Compensated(String),
}

pub trait Compensator {
    fn compensate(&self, step: &str) -> Result<(), String>;
}

#[derive(Debug, PartialEq)]
pub struct Report {
    pub compensated: Vec<String>,
    pub failed: Option<(String, String)>,
}

pub fn recover(journal: &[Entry], compensator: &dyn Compensator) -> Report {
    let mut pending: Vec<&String> = Vec::new();
    for entry in journal {
        match entry {
            Entry::Started(step) => pending.push(step),
            Entry::Compensated(step) => pending.retain(|started| *started != step),
            Entry::Done(_) => {}
        }
    }
    let mut report = Report { compensated: Vec::new(), failed: None };
    for step in pending.into_iter().rev() {
        match compensator.compensate(step) {
            Ok(()) => report.compensated.push(step.clone()),
            Err(error) => {
                report.failed = Some((step.clone(), error));
                return report;
            }
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Recorder {
        calls: Mutex<Vec<String>>,
        broken: HashSet<&'static str>,
    }

    impl Compensator for Recorder {
        fn compensate(&self, step: &str) -> Result<(), String> {
            self.calls.lock().unwrap().push(step.to_string());
            if self.broken.contains(step) { Err(format!("{step}: сервис недоступен")) } else { Ok(()) }
        }
    }

    fn started(name: &str) -> Entry {
        Entry::Started(name.into())
    }

    fn done(name: &str) -> Entry {
        Entry::Done(name.into())
    }

    fn undone(name: &str) -> Entry {
        Entry::Compensated(name.into())
    }

    fn calls(recorder: &Recorder) -> Vec<String> {
        recorder.calls.lock().unwrap().clone()
    }

    #[test]
    fn started_steps_are_undone_in_reverse_order() {
        let journal = vec![started("freeze"), done("freeze"), started("hide"), done("hide"), started("revoke"), done("revoke")];
        let recorder = Recorder::default();
        let report = recover(&journal, &recorder);
        assert_eq!(calls(&recorder), vec!["revoke", "hide", "freeze"], "обратный порядок начала");
        assert_eq!(report, Report { compensated: vec!["revoke".into(), "hide".into(), "freeze".into()], failed: None }, "отчёт");
    }

    #[test]
    fn a_step_that_crashed_midway_is_undone_too() {
        let journal = vec![started("freeze"), done("freeze"), started("hide")];
        let recorder = Recorder::default();
        recover(&journal, &recorder);
        assert_eq!(calls(&recorder), vec!["hide", "freeze"], "hide начат, но не завершён: что-то он мог успеть сделать, откатываем и его");
    }

    #[test]
    fn already_compensated_steps_are_not_undone_again() {
        let journal = vec![started("freeze"), done("freeze"), started("hide"), done("hide"), undone("hide")];
        let recorder = Recorder::default();
        let report = recover(&journal, &recorder);
        assert_eq!(calls(&recorder), vec!["freeze"], "hide уже откатан в прошлый раз, второй откат вернёт деньги дважды");
        assert_eq!(report.compensated, vec!["freeze"], "в отчёте только новое");
        let again = recover(&[started("a"), done("a"), undone("a")], &recorder);
        assert!(again.compensated.is_empty() && again.failed.is_none(), "когда всё откатано, делать нечего");
    }

    #[test]
    fn the_first_failure_stops_recovery_and_is_reported() {
        let journal = vec![started("freeze"), done("freeze"), started("hide"), done("hide"), started("revoke"), done("revoke")];
        let recorder = Recorder { broken: HashSet::from(["hide"]), ..Default::default() };
        let report = recover(&journal, &recorder);
        assert_eq!(calls(&recorder), vec!["revoke", "hide"], "после неудачи на hide дальше не идём: порядок откатов важнее скорости");
        assert_eq!(report.compensated, vec!["revoke"], "откачен только revoke");
        assert_eq!(report.failed, Some(("hide".to_string(), "hide: сервис недоступен".to_string())), "названы шаг и причина");
    }

    #[test]
    fn an_empty_journal_has_nothing_to_do() {
        let recorder = Recorder::default();
        assert_eq!(recover(&[], &recorder), Report { compensated: vec![], failed: None }, "пустой журнал");
    }
}
