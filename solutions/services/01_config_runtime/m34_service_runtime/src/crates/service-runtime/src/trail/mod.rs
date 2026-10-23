use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};

type Write = Arc<dyn Fn(&str) + Send + Sync>;

#[derive(Clone, Default)]
pub struct Trail {
    write: Option<Write>,
    kept: Arc<Mutex<Vec<String>>>,
}

impl Trail {
    pub fn quiet() -> Trail {
        Trail::default()
    }

    pub fn new(write: impl Fn(&str) + Send + Sync + 'static) -> Trail {
        Trail {
            write: Some(Arc::new(write)),
            kept: Arc::default(),
        }
    }

    pub fn stderr(service: &'static str) -> Trail {
        Trail::new(move |line| eprintln!("{service}: {line}"))
    }

    pub fn note(&self, line: &str) {
        self.kept
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(line.to_string());
        if let Some(write) = &self.write {
            write(line);
        }
    }

    pub fn lines(&self) -> Vec<String> {
        self.kept
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl fmt::Debug for Trail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Trail")
    }
}

impl PartialEq for Trail {
    fn eq(&self, other: &Trail) -> bool {
        Arc::ptr_eq(&self.kept, &other.kept)
    }
}

impl Eq for Trail {}
