use std::fs;
use std::path::{Path, PathBuf};
use std::env;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    pub fn new() -> TempDir {
        let unique = format!("tst11_journal_{}_{}", std::process::id(), COUNTER.fetch_add(1, Ordering::SeqCst));
        let path = env::temp_dir().join(unique);
        fs::create_dir_all(&path).expect("не удалось создать временную папку");
        TempDir { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

impl Default for TempDir {
    fn default() -> Self {
        TempDir::new()
    }
}

pub struct Journal {
    file: PathBuf,
}

impl Journal {
    pub fn open(dir: &Path) -> Journal {
        Journal { file: dir.join("journal.log") }
    }

    pub fn append(&self, line: &str) {
        use std::io::Write;
        let mut file = fs::OpenOptions::new().create(true).append(true).open(&self.file).expect("не удалось открыть журнал");
        writeln!(file, "{line}").expect("не удалось записать строку");
    }

    pub fn lines(&self) -> Vec<String> {
        fs::read_to_string(&self.file).map(|text| text.lines().map(str::to_string).collect()).unwrap_or_default()
    }
}

fn main() {
    let dir = TempDir::new();
    let journal = Journal::open(dir.path());
    journal.append("запуск");
    println!("{:?}", journal.lines());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appended_lines_come_back_in_order() {
        let dir = TempDir::new();
        let journal = Journal::open(dir.path());
        journal.append("a");
        journal.append("b");
        assert_eq!(journal.lines(), vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn fresh_journal_is_empty() {
        let dir = TempDir::new();
        let journal = Journal::open(dir.path());
        assert!(journal.lines().is_empty(), "в новой папке журнал пуст, чужих строк быть не должно");
    }

    #[test]
    fn each_test_gets_its_own_directory() {
        let paths: Vec<PathBuf> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..8)
                .map(|n| {
                    scope.spawn(move || {
                        let dir = TempDir::new();
                        Journal::open(dir.path()).append(&format!("поток {n}"));
                        assert_eq!(Journal::open(dir.path()).lines(), vec![format!("поток {n}")], "в папке чужие строки");
                        dir.path().to_path_buf()
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().expect("поток упал")).collect()
        });
        let mut unique = paths.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), 8, "каждому TempDir нужен свой путь");
    }

    #[test]
    fn directory_disappears_with_the_guard() {
        let path = {
            let dir = TempDir::new();
            Journal::open(dir.path()).append("x");
            assert!(dir.path().exists());
            dir.path().to_path_buf()
        };
        assert!(!path.exists(), "после drop папка со всем содержимым должна исчезнуть");
    }

    #[test]
    fn directory_disappears_even_when_the_test_panics() {
        let seen = std::sync::Mutex::new(None);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let dir = TempDir::new();
            *seen.lock().unwrap() = Some(dir.path().to_path_buf());
            panic!("тест упал");
        }));
        assert!(result.is_err());
        let path = seen.lock().unwrap().clone().expect("путь не запомнен");
        assert!(!path.exists(), "падение теста не должно оставлять мусор: уборка живёт в Drop");
    }
}
