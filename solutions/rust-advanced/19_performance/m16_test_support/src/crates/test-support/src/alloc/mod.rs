use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

pub struct CountingAlloc;

fn bump() {
    let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
}

fn current() -> usize {
    ALLOCATIONS.try_with(Cell::get).unwrap_or(0)
}

#[allow(unsafe_code)]
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        bump();
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        bump();
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        bump();
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

pub fn allocations<R>(f: impl FnOnce() -> R) -> (R, usize) {
    let before = current();
    let result = f();
    (result, current().saturating_sub(before))
}
