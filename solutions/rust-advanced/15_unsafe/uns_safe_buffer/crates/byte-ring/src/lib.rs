//! Кольцевой буфер байтов с unsafe внутри и безопасным API.

use std::alloc::{self, Layout};
use std::ptr::{self, NonNull};
use std::slice;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Full;

pub struct ByteRing {
    ptr: NonNull<u8>,
    capacity: usize,
    head: usize,
    len: usize,
}

// SAFETY: ByteRing единолично владеет своей памятью, как Vec<u8>; ни указателей наружу, ни общих ссылок нет.
unsafe impl Send for ByteRing {}
// SAFETY: через &ByteRing можно только читать (len, peek, as_slices), любая запись требует &mut.
unsafe impl Sync for ByteRing {}

impl ByteRing {
    pub fn with_capacity(capacity: usize) -> ByteRing {
        let ptr = if capacity == 0 {
            NonNull::dangling()
        } else {
            let layout = Layout::array::<u8>(capacity).expect("слишком большая ёмкость");
            // SAFETY: размер раскладки не нулевой.
            let raw = unsafe { alloc::alloc(layout) };
            NonNull::new(raw).unwrap_or_else(|| alloc::handle_alloc_error(layout))
        };
        ByteRing { ptr, capacity, head: 0, len: 0 }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn is_full(&self) -> bool {
        self.len == self.capacity
    }

    fn wrap(&self, index: usize) -> usize {
        index % self.capacity
    }

    pub fn push(&mut self, byte: u8) -> Result<(), Full> {
        if self.is_full() {
            return Err(Full);
        }
        let at = self.wrap(self.head + self.len);
        // SAFETY: at меньше ёмкости (взято по модулю), память выделена, и этот байт пока не хранит данных.
        unsafe { self.ptr.as_ptr().add(at).write(byte) };
        self.len += 1;
        Ok(())
    }

    pub fn pop(&mut self) -> Option<u8> {
        let byte = self.peek()?;
        self.head = self.wrap(self.head + 1);
        self.len -= 1;
        Some(byte)
    }

    pub fn peek(&self) -> Option<u8> {
        if self.len == 0 {
            return None;
        }
        // SAFETY: len больше нуля, head меньше ёмкости, и байт по head уже записан.
        Some(unsafe { self.ptr.as_ptr().add(self.head).read() })
    }

    pub fn write(&mut self, data: &[u8]) -> usize {
        let count = data.len().min(self.capacity - self.len);
        if count == 0 {
            return 0;
        }
        let tail = self.wrap(self.head + self.len);
        let first = count.min(self.capacity - tail);
        // SAFETY: области [tail, tail + first) и [0, count - first) лежат внутри буфера и не пересекаются,
        // потому что count не больше свободного места; data это чужой срез и с буфером не пересекается.
        unsafe {
            ptr::copy_nonoverlapping(data.as_ptr(), self.ptr.as_ptr().add(tail), first);
            ptr::copy_nonoverlapping(data.as_ptr().add(first), self.ptr.as_ptr(), count - first);
        }
        self.len += count;
        count
    }

    pub fn read(&mut self, out: &mut [u8]) -> usize {
        let count = out.len().min(self.len);
        if count == 0 {
            return 0;
        }
        let first = count.min(self.capacity - self.head);
        // SAFETY: читаются только записанные байты: [head, head + first) и [0, count - first) входят в len;
        // out это &mut и с буфером не пересекается.
        unsafe {
            ptr::copy_nonoverlapping(self.ptr.as_ptr().add(self.head), out.as_mut_ptr(), first);
            ptr::copy_nonoverlapping(self.ptr.as_ptr(), out.as_mut_ptr().add(first), count - first);
        }
        self.head = self.wrap(self.head + count);
        self.len -= count;
        count
    }

    pub fn as_slices(&self) -> (&[u8], &[u8]) {
        if self.len == 0 {
            return (&[], &[]);
        }
        let first = self.len.min(self.capacity - self.head);
        // SAFETY: оба диапазона входят в len, а значит записаны; пока живёт &self, записи невозможны.
        unsafe {
            (
                slice::from_raw_parts(self.ptr.as_ptr().add(self.head), first),
                slice::from_raw_parts(self.ptr.as_ptr(), self.len - first),
            )
        }
    }

    pub fn discard(&mut self, count: usize) -> usize {
        let count = count.min(self.len);
        if count > 0 {
            self.head = self.wrap(self.head + count);
            self.len -= count;
        }
        count
    }

    pub fn clear(&mut self) {
        self.head = 0;
        self.len = 0;
    }
}

impl Drop for ByteRing {
    fn drop(&mut self) {
        if self.capacity > 0 {
            let layout = Layout::array::<u8>(self.capacity).expect("раскладка уже была построена при создании");
            // SAFETY: память выделена в with_capacity с этой же раскладкой и нигде не освобождалась.
            unsafe { alloc::dealloc(self.ptr.as_ptr(), layout) };
        }
    }
}
