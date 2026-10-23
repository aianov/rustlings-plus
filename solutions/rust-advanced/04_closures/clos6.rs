use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

pub fn start_workers(names: Vec<String>, log: Arc<Mutex<Vec<String>>>) -> Vec<JoinHandle<()>> {
    names
        .into_iter()
        .map(|name| {
            let log = Arc::clone(&log);
            thread::spawn(move || {
                log.lock().expect("журнал отравлен").push(format!("hello {name}"));
            })
        })
        .collect()
}

pub fn greeters(names: &[String]) -> Vec<Box<dyn Fn() -> String>> {
    names
        .iter()
        .map(|name| {
            let name = name.clone();
            Box::new(move || format!("hi {name}")) as Box<dyn Fn() -> String>
        })
        .collect()
}

fn main() {
    let log = Arc::new(Mutex::new(Vec::new()));
    for h in start_workers(vec!["ann".into()], Arc::clone(&log)) {
        h.join().expect("поток упал");
    }
    println!("{:?}", log.lock().expect("журнал отравлен"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_worker_writes_its_line() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let names: Vec<String> = ["ann", "bob", "cat", "dan"].iter().map(|s| s.to_string()).collect();
        let handles = start_workers(names, Arc::clone(&log));
        assert_eq!(handles.len(), 4, "по потоку на имя");
        for h in handles {
            h.join().expect("поток упал");
        }
        let mut lines = log.lock().expect("журнал отравлен").clone();
        lines.sort();
        assert_eq!(lines, vec!["hello ann", "hello bob", "hello cat", "hello dan"]);
    }

    #[test]
    fn no_extra_handles_are_left_behind() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let handles = start_workers(vec!["ann".into(), "bob".into()], Arc::clone(&log));
        for h in handles {
            h.join().expect("поток упал");
        }
        assert_eq!(Arc::strong_count(&log), 1, "после завершения потоков у журнала должен остаться один владелец");
    }

    #[test]
    fn greeters_outlive_the_source() {
        let fs = {
            let names = vec!["ann".to_string(), "bob".to_string()];
            greeters(&names)
        };
        let got: Vec<String> = fs.iter().map(|f| f()).collect();
        assert_eq!(got, vec!["hi ann", "hi bob"], "замыкания хранят собственные копии имён");
        assert_eq!(fs[0](), "hi ann", "и вызываются сколько угодно раз");
    }
}
