use std::error::Error;
use std::fmt;
use std::thread;

use std::sync::Arc;

#[derive(Debug)]
pub struct Rejected {
    pub reason: Arc<str>,
}

impl fmt::Display for Rejected {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "rejected: {}", self.reason)
    }
}

impl Error for Rejected {}

fn parse_positive(input: &str) -> Result<u32, Box<dyn Error + Send + Sync>> {
    let n: u32 = input.trim().parse()?;
    if n == 0 {
        return Err(Box::new(Rejected { reason: Arc::from("zero is not allowed") }));
    }
    Ok(n)
}

pub fn check_in_thread(input: String) -> thread::JoinHandle<Result<u32, Box<dyn Error + Send + Sync>>> {
    thread::spawn(move || parse_positive(&input))
}

fn main() {
    println!("{:?}", check_in_thread("12".to_string()).join().map(|r| r.map_err(|e| e.to_string())));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn good_input_crosses_the_thread() {
        assert_eq!(check_in_thread("42".into()).join().unwrap().unwrap(), 42);
    }

    #[test]
    fn errors_cross_the_thread_too() {
        let err = check_in_thread("0".into()).join().unwrap().unwrap_err();
        assert_eq!(err.to_string(), "rejected: zero is not allowed");
        assert!(err.downcast_ref::<Rejected>().is_some(), "тип ошибки не потерялся по дороге");
        let err = check_in_thread("abc".into()).join().unwrap().unwrap_err();
        assert_eq!(err.to_string(), "invalid digit found in string");
    }

    #[test]
    fn many_threads_in_parallel() {
        let handles: Vec<_> = ["1", "2", "0", "x"].iter().map(|s| check_in_thread(s.to_string())).collect();
        let results: Vec<bool> = handles.into_iter().map(|h| h.join().unwrap().is_ok()).collect();
        assert_eq!(results, vec![true, true, false, false]);
    }
}
