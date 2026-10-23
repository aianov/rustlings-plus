// Нужно быстро обработать большой срез, который лежит у нас в переменной. Через `thread::spawn` его
// не отдашь: поток требует `'static`, а срез живёт только в этой функции. Для таких случаев есть
// потоки с областью: они обязаны закончиться до выхода из неё, поэтому могут брать данные взаймы.
// TODO: Допиши функции на `thread::scope`, без `Arc`, `Mutex` и копирования данных.
use std::thread;

pub fn chunk_sums(data: &[u64], parts: usize) -> Vec<u64> {
    todo!("срез делится на куски по ceil(len / parts) элементов, сумма каждого считается в своём потоке")
}

pub fn for_each_parallel<T: Sync>(items: &[T], parts: usize, f: impl Fn(&T) + Sync) {
    todo!("те же куски, f вызывается для каждого элемента из потока своего куска")
}

pub fn square_in_place(out: &mut [u64], parts: usize) {
    todo!("каждый поток возводит в квадрат свой кусок среза на месте")
}

fn main() {
    println!("{:?}", chunk_sums(&[1, 2, 3, 4], 2));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    #[test]
    fn chunk_sums_follow_the_chunk_order() {
        let data: Vec<u64> = (1..=10).collect();
        assert_eq!(chunk_sums(&data, 3), vec![10, 26, 19], "куски по 4, 4 и 2 элемента");
        assert_eq!(chunk_sums(&data, 1), vec![55]);
        assert_eq!(chunk_sums(&data, 100).len(), 10, "кусков не больше, чем элементов");
        assert!(chunk_sums(&[], 4).is_empty());
        assert_eq!(chunk_sums(&data, 0), vec![55], "ноль частей не должен паниковать");
    }

    #[test]
    fn the_callback_runs_for_every_item() {
        let seen = AtomicUsize::new(0);
        let items: Vec<u32> = (0..37).collect();
        for_each_parallel(&items, 5, |_| {
            seen.fetch_add(1, Ordering::Relaxed);
        });
        assert_eq!(seen.load(Ordering::Relaxed), 37);
    }

    #[test]
    fn chunks_really_run_at_the_same_time() {
        let started = AtomicUsize::new(0);
        let too_slow = AtomicBool::new(false);
        let items = [1, 2, 3, 4];
        for_each_parallel(&items, 4, |_| {
            started.fetch_add(1, Ordering::SeqCst);
            let deadline = Instant::now() + Duration::from_secs(2);
            while started.load(Ordering::SeqCst) < 4 {
                if Instant::now() > deadline {
                    too_slow.store(true, Ordering::SeqCst);
                    return;
                }
                thread::yield_now();
            }
        });
        assert!(!too_slow.load(Ordering::SeqCst), "четыре куска должны работать одновременно, а не по очереди");
    }

    #[test]
    fn slices_are_modified_in_place_without_copies() {
        let mut data: Vec<u64> = (1..=9).collect();
        let before = data.as_ptr();
        square_in_place(&mut data, 4);
        assert_eq!(data, vec![1, 4, 9, 16, 25, 36, 49, 64, 81]);
        assert_eq!(data.as_ptr(), before, "данные меняются на месте");
        let mut empty: Vec<u64> = Vec::new();
        square_in_place(&mut empty, 3);
    }
}
