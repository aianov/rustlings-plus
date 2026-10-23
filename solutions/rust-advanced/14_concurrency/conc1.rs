use std::any::Any;
use std::thread;

pub fn sum_in_thread(data: Vec<u32>) -> u64 {
    thread::spawn(move || data.iter().map(|&n| u64::from(n)).sum())
        .join()
        .expect("поток суммы упал")
}

pub fn map_in_threads<T: Send + 'static, R: Send + 'static>(items: Vec<T>, f: fn(T) -> R) -> Vec<R> {
    let handles: Vec<_> = items.into_iter().map(|item| thread::spawn(move || f(item))).collect();
    handles.into_iter().map(|handle| handle.join().expect("поток упал")).collect()
}

pub fn panic_message(payload: &(dyn Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_string()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "неизвестная паника".to_string()
    }
}

pub fn run_catching(job: fn() -> i32) -> Result<i32, String> {
    thread::spawn(job).join().map_err(|payload| panic_message(payload.as_ref()))
}

fn main() {
    println!("{}", sum_in_thread(vec![1, 2, 3]));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn sum_comes_back_from_the_thread() {
        assert_eq!(sum_in_thread(vec![1, 2, 3, 4]), 10);
        assert_eq!(sum_in_thread(vec![]), 0);
        assert_eq!(sum_in_thread(vec![u32::MAX, u32::MAX]), 2 * u64::from(u32::MAX), "сумму считай в u64, иначе переполнение");
    }

    #[test]
    fn results_keep_the_input_order() {
        assert_eq!(map_in_threads(vec![3, 1, 2], |n| n * 10), vec![30, 10, 20], "порядок результатов должен совпасть с порядком входа");
        assert!(map_in_threads(Vec::<u8>::new(), |n| n).is_empty());
    }

    #[test]
    fn every_item_gets_its_own_thread() {
        let ids = map_in_threads(vec![(); 4], |()| thread::current().id());
        let unique: HashSet<_> = ids.iter().collect();
        assert_eq!(unique.len(), 4, "на каждый элемент нужен свой поток");
        assert!(!ids.contains(&thread::current().id()), "работа должна идти не в вызывающем потоке");
    }

    #[test]
    fn panic_text_is_extracted_for_str_and_string() {
        let a = thread::spawn(|| panic!("boom")).join().unwrap_err();
        assert_eq!(panic_message(a.as_ref()), "boom");
        let n = 7;
        let b = thread::spawn(move || panic!("код {n}")).join().unwrap_err();
        assert_eq!(panic_message(b.as_ref()), "код 7", "паника с форматированием несёт String, а не &str");
        let c = thread::spawn(|| std::panic::panic_any(5_u8)).join().unwrap_err();
        assert_eq!(panic_message(c.as_ref()), "неизвестная паника");
    }

    #[test]
    fn a_panicking_job_becomes_an_error() {
        assert_eq!(run_catching(|| 7), Ok(7));
        assert_eq!(run_catching(|| panic!("нет такого пользователя")), Err("нет такого пользователя".to_string()));
    }
}
