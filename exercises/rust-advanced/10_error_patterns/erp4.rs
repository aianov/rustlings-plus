// Приложение должно сообщать об ошибке одной строкой: «верхний уровень: ниже: настоящая причина». Две ошибки
// ниже обёрнуты друг в друга через `source`. Сейчас в отчёте всё повторяется по два раза, как будто
// каждый уровень решил рассказать всю историю сам. Ещё нужны две функции обхода цепочки: полная строка
// и корневая причина.
// TODO: Реши, кто отвечает за какую часть сообщения, и напиши `chain_line` и `root_cause`. Ожидаемая
// строка для теста: `load profile: read data.bin: disk is full`.
use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub struct DiskFull;

impl fmt::Display for DiskFull {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "disk is full")
    }
}

impl Error for DiskFull {}

#[derive(Debug)]
pub struct ReadFailed {
    pub path: String,
    pub source: DiskFull,
}

impl Error for ReadFailed {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.source)
    }
}

#[derive(Debug)]
pub struct LoadProfile {
    pub source: ReadFailed,
}

impl Error for LoadProfile {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.source)
    }
}

impl fmt::Display for ReadFailed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "read {}: {}", self.path, self.source)
    }
}

impl fmt::Display for LoadProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "load profile: {}", self.source)
    }
}

pub fn chain_line(err: &(dyn Error + 'static)) -> String {
    todo!("все уровни через двоеточие")
}

pub fn root_cause<'a>(err: &'a (dyn Error + 'static)) -> &'a (dyn Error + 'static) {
    todo!("самая глубокая причина")
}

fn main() {
    let err = LoadProfile { source: ReadFailed { path: "data.bin".into(), source: DiskFull } };
    println!("{}", chain_line(&err));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack() -> LoadProfile {
        LoadProfile { source: ReadFailed { path: "data.bin".into(), source: DiskFull } }
    }

    #[test]
    fn every_level_speaks_once() {
        assert_eq!(chain_line(&stack()), "load profile: read data.bin: disk is full", "каждая часть сообщения ровно один раз");
    }

    #[test]
    fn display_is_only_about_its_own_level() {
        assert_eq!(stack().to_string(), "load profile", "Display верхнего уровня не пересказывает причину");
        assert_eq!(stack().source.to_string(), "read data.bin");
    }

    #[test]
    fn works_for_a_single_error_and_for_a_boxed_one() {
        assert_eq!(chain_line(&DiskFull), "disk is full");
        let boxed: Box<dyn Error> = Box::new(stack());
        assert_eq!(chain_line(boxed.as_ref()), "load profile: read data.bin: disk is full");
    }

    #[test]
    fn root_cause_is_the_deepest_error() {
        let err = stack();
        let root = root_cause(&err);
        assert_eq!(root.to_string(), "disk is full");
        assert!(root.downcast_ref::<DiskFull>().is_some(), "корень остаётся настоящим DiskFull");
        assert_eq!(root_cause(&DiskFull).to_string(), "disk is full", "у ошибки без причины корень она сама");
    }
}
