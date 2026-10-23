use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);

pub struct CountingAlloc;

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(new_size, Ordering::Relaxed);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Counts {
    pub allocs: usize,
    pub bytes: usize,
}

pub fn counts() -> Counts {
    Counts { allocs: ALLOCS.load(Ordering::Relaxed), bytes: BYTES.load(Ordering::Relaxed) }
}

pub fn measure<R>(f: impl FnOnce() -> R) -> (R, Counts) {
    let before = counts();
    let result = f();
    let after = counts();
    (result, Counts { allocs: after.allocs - before.allocs, bytes: after.bytes - before.bytes })
}

fn main() {
    let (v, c) = measure(|| vec![1u32; 10]);
    println!("{} элементов, {:?}", v.len(), c);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[global_allocator]
    static GLOBAL: CountingAlloc = CountingAlloc;

    #[test]
    fn counter_sees_real_allocations() {
        let (v, c) = measure(|| Vec::<u64>::with_capacity(100));
        assert_eq!((c.allocs, c.bytes), (1, 800), "with_capacity(100) для u64 это одна аллокация на 800 байт");
        drop(v);

        let (s, c) = measure(|| String::from("hello"));
        assert_eq!((c.allocs, c.bytes), (1, 5), "строка из пяти байт");
        drop(s);

        let (b, c) = measure(|| Box::new(7u64));
        assert_eq!((c.allocs, c.bytes), (1, 8), "Box<u64> это 8 байт");
        drop(b);

        let (sum, c) = measure(|| [1u32, 2, 3, 4].iter().sum::<u32>());
        assert_eq!((sum, c.allocs, c.bytes), (10, 0, 0), "сумма по массиву на стеке ничего не просит у аллокатора");

        let (v, c) = measure(|| {
            let mut v = Vec::new();
            for n in 0..5u32 {
                v.push(n);
            }
            v
        });
        assert_eq!(c.allocs, 2, "пять push в пустой Vec<u32>: выделение на 4 элемента и расширение до 8");
        assert_eq!(c.bytes, 16 + 32, "сначала 16 байт, потом realloc до 32: считаем новый размер");
        drop(v);

        let (v, c) = measure(|| vec![0u8; 64]);
        assert_eq!((c.allocs, c.bytes), (1, 64), "vec![0; n] идёт через alloc_zeroed: его тоже надо считать");
        drop(v);

        let before = counts();
        let v: Vec<u8> = Vec::with_capacity(1000);
        let after = counts();
        assert!(after.allocs > before.allocs && after.bytes >= before.bytes + 1000, "счётчики общие и только растут");
        drop(v);
    }
}
