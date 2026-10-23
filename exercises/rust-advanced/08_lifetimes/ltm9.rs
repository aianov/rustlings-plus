// Загрузчик опрашивает несколько источников сразу, потому что каждый отвечает медленно. `load_all`
// запускает по потоку на источник и собирает ответы в исходном порядке. Не собирается: «borrowed data
// escapes outside of function» (E0521). `thread::spawn`
// требует, чтобы замыкание не держало короткоживущих ссылок: поток может пережить функцию, которая его
// запустила. А тут источники лежат у вызывающего, клонировать `dyn Source` нельзя. Раз мы сами ждём все
// потоки до выхода из функции, нужен способ сказать об этом компилятору.
// TODO: Почини `load_all`, не меняя сигнатуру и не отказываясь от потоков (тест проверяет, что источники
// опрашиваются одновременно).
use std::thread;

pub trait Source: Sync {
    fn load(&self) -> String;
}

pub fn load_all(sources: &[&dyn Source]) -> Vec<String> {
    let mut handles = Vec::new();
    for source in sources {
        handles.push(thread::spawn(|| source.load()));
    }
    handles.into_iter().map(|h| h.join().expect("поток источника упал")).collect()
}

fn main() {
    struct Fixed;
    impl Source for Fixed {
        fn load(&self) -> String {
            "ok".to_string()
        }
    }
    println!("{:?}", load_all(&[&Fixed, &Fixed]));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    struct Slow<'a> {
        name: &'a str,
        running: &'a AtomicUsize,
        peak: &'a AtomicUsize,
    }

    impl Source for Slow<'_> {
        fn load(&self) -> String {
            let now = self.running.fetch_add(1, Ordering::SeqCst) + 1;
            self.peak.fetch_max(now, Ordering::SeqCst);
            thread::sleep(Duration::from_millis(60));
            self.running.fetch_sub(1, Ordering::SeqCst);
            format!("data from {}", self.name)
        }
    }

    #[test]
    fn answers_come_back_in_source_order() {
        let (running, peak) = (AtomicUsize::new(0), AtomicUsize::new(0));
        let slow: Vec<Slow> = ["a", "b", "c", "d"].iter().map(|name| Slow { name, running: &running, peak: &peak }).collect();
        let list: Vec<&dyn Source> = slow.iter().map(|s| s as &dyn Source).collect();
        assert_eq!(load_all(&list), vec!["data from a", "data from b", "data from c", "data from d"]);
    }

    #[test]
    fn sources_are_polled_at_the_same_time() {
        let (running, peak) = (AtomicUsize::new(0), AtomicUsize::new(0));
        let slow: Vec<Slow> = ["a", "b", "c", "d"].iter().map(|name| Slow { name, running: &running, peak: &peak }).collect();
        let list: Vec<&dyn Source> = slow.iter().map(|s| s as &dyn Source).collect();
        load_all(&list);
        assert!(peak.load(Ordering::SeqCst) >= 2, "источники опрашивались по очереди: должно быть несколько одновременно, по потоку на источник");
    }

    #[test]
    fn no_sources_no_answers() {
        assert!(load_all(&[]).is_empty());
    }
}
