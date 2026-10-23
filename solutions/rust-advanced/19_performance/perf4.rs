use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::cell::Cell;

thread_local! {
    static ALLOCS: Cell<usize> = const { Cell::new(0) };
    static BYTES: Cell<usize> = const { Cell::new(0) };
}

pub struct CountingAlloc;

fn bump(size: usize) {
    let _ = ALLOCS.try_with(|n| n.set(n.get() + 1));
    let _ = BYTES.try_with(|n| n.set(n.get() + size));
}

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        bump(layout.size());
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        bump(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        bump(new_size);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Counts {
    pub allocs: usize,
    pub bytes: usize,
}

pub fn counts() -> Counts {
    Counts { allocs: ALLOCS.get(), bytes: BYTES.get() }
}

pub fn measure<R>(f: impl FnOnce() -> R) -> (R, Counts) {
    let before = counts();
    let result = f();
    let after = counts();
    (result, Counts { allocs: after.allocs - before.allocs, bytes: after.bytes - before.bytes })
}

pub fn sum_even(values: &[u32]) -> u64 {
    values.iter().filter(|&&v| v % 2 == 0).map(|&v| u64::from(v)).sum()
}

fn main() {
    let (sum, c) = measure(|| sum_even(&[1, 2, 3, 4]));
    println!("{sum}, {c:?}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::time::{Duration, Instant};

    #[global_allocator]
    static GLOBAL: CountingAlloc = CountingAlloc;

    fn with_noise<R>(f: impl FnOnce() -> R) -> R {
        let stop = Arc::new(AtomicBool::new(false));
        let spins = Arc::new(AtomicUsize::new(0));
        let noisy = {
            let (stop, spins) = (stop.clone(), spins.clone());
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    let buffer: Vec<u8> = Vec::with_capacity(32);
                    std::hint::black_box(&buffer);
                    spins.fetch_add(1, Ordering::Relaxed);
                }
            })
        };
        while spins.load(Ordering::Relaxed) < 1_000 {
            std::thread::yield_now();
        }
        let result = f();
        stop.store(true, Ordering::Relaxed);
        noisy.join().expect("шумный поток упал");
        result
    }

    fn busy_for(duration: Duration) -> u64 {
        let start = Instant::now();
        let mut acc = 0u64;
        while start.elapsed() < duration {
            acc = acc.wrapping_add(sum_even(&[2, 4, 6, 7]));
        }
        acc
    }

    #[test]
    fn quiet_code_counts_zero_even_next_to_a_noisy_thread() {
        let (_, c) = with_noise(|| measure(|| busy_for(Duration::from_millis(5))));
        assert_eq!(c.allocs, 0, "замер увидел {} аллокаций чужого потока: счётчик должен быть свой у каждого потока", c.allocs);
    }

    #[test]
    fn own_allocations_are_still_counted() {
        let (v, c) = with_noise(|| measure(|| Vec::<u32>::with_capacity(10)));
        assert_eq!((c.allocs, c.bytes), (1, 40), "собственные аллокации потока должны считаться точно");
        drop(v);
    }

    #[test]
    fn each_thread_sees_only_itself() {
        let (_, here) = measure(|| {
            let from_other = std::thread::spawn(|| measure(|| Vec::<u8>::with_capacity(500)).1).join().expect("поток упал");
            assert_eq!((from_other.allocs, from_other.bytes), (1, 500), "внутри потока видна его аллокация");
        });
        assert!(here.bytes < 500, "аллокации другого потока не должны попасть в счётчик этого ({} байт)", here.bytes);
    }
}
