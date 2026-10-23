use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub struct Contextual {
    message: String,
    source: Box<dyn Error + Send + Sync>,
}

impl fmt::Display for Contextual {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for Contextual {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&*self.source)
    }
}

pub trait Context<T> {
    fn context(self, message: &str) -> Result<T, Contextual>;
    fn with_context<F: FnOnce() -> String>(self, make: F) -> Result<T, Contextual>;
}

impl<T, E> Context<T> for Result<T, E>
where
    E: Error + Send + Sync + 'static,
{
    fn context(self, message: &str) -> Result<T, Contextual> {
        self.map_err(|e| Contextual { message: message.to_string(), source: Box::new(e) })
    }

    fn with_context<F: FnOnce() -> String>(self, make: F) -> Result<T, Contextual> {
        self.map_err(|e| Contextual { message: make(), source: Box::new(e) })
    }
}

fn main() {
    let err = "70000".parse::<u16>().context("reading port").unwrap_err();
    println!("{err}: {}", err.source().map(|s| s.to_string()).unwrap_or_default());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn ok_passes_through() {
        assert_eq!("42".parse::<u16>().context("reading port").unwrap(), 42);
    }

    #[test]
    fn context_adds_a_message_and_keeps_the_cause() {
        let err = "x".parse::<u16>().context("reading port").unwrap_err();
        assert_eq!(err.to_string(), "reading port");
        let cause = err.source().expect("причина сохранена");
        assert_eq!(cause.to_string(), "invalid digit found in string");
        assert!(cause.downcast_ref::<std::num::ParseIntError>().is_some(), "внутри настоящая ParseIntError");
    }

    #[test]
    fn contexts_stack_into_a_chain() {
        let err = "x"
            .parse::<u16>()
            .context("reading port")
            .context("starting server")
            .unwrap_err();
        let mut texts = vec![err.to_string()];
        let mut current = err.source();
        while let Some(e) = current {
            texts.push(e.to_string());
            current = e.source();
        }
        assert_eq!(texts, vec!["starting server", "reading port", "invalid digit found in string"]);
    }

    #[test]
    fn with_context_is_lazy() {
        let made = Cell::new(0);
        let ok: Result<u8, std::num::ParseIntError> = "7".parse();
        let value = ok
            .with_context(|| {
                made.set(made.get() + 1);
                "never needed".to_string()
            })
            .unwrap();
        assert_eq!(value, 7);
        assert_eq!(made.get(), 0, "ошибки не было, строка не строится");
        let err = "".parse::<u8>()
            .with_context(|| {
                made.set(made.get() + 1);
                format!("reading field {}", 3)
            })
            .unwrap_err();
        assert_eq!(err.to_string(), "reading field 3");
        assert_eq!(made.get(), 1);
    }

    #[test]
    fn contextual_can_cross_threads() {
        let err = "x".parse::<u8>().context("in worker").unwrap_err();
        let text = std::thread::spawn(move || err.to_string()).join().unwrap();
        assert_eq!(text, "in worker");
    }
}
