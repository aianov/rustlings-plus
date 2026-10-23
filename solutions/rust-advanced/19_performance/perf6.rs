use std::fmt::Write;

pub fn render_lines(rows: &[(u32, &str)]) -> String {
    let mut out = String::with_capacity(rows.iter().map(|(_, name)| name.len() + 12).sum());
    for (id, name) in rows {
        let _ = write!(out, "{id}:");
        out.extend(name.chars().flat_map(char::to_uppercase));
        out.push('\n');
    }
    out
}

pub fn squares(n: u32) -> Vec<u64> {
    (0..n).map(|i| u64::from(i) * u64::from(i)).collect()
}

pub struct Encoder {
    buf: Vec<u8>,
}

impl Encoder {
    pub fn new() -> Encoder {
        Encoder { buf: Vec::with_capacity(64) }
    }

    pub fn encode(&mut self, message: &str) -> &[u8] {
        self.buf.clear();
        self.buf.extend_from_slice(b"MSG:");
        self.buf.extend_from_slice(message.as_bytes());
        self.buf.push(b'\n');
        &self.buf
    }
}

impl Default for Encoder {
    fn default() -> Self {
        Encoder::new()
    }
}

fn main() {
    println!("{}", render_lines(&[(1, "ann"), (2, "bob")]));
    println!("{}", squares(5).len());
}

#[cfg(test)]
mod tests {
    use super::*;
    #[allow(dead_code)]
    mod counting {
        use std::alloc::{GlobalAlloc, Layout, System};
        use std::cell::Cell;

        thread_local! {
            static ALLOCS: Cell<usize> = const { Cell::new(0) };
            static BYTES: Cell<usize> = const { Cell::new(0) };
        }

        pub struct Counting;

        fn bump(size: usize) {
            let _ = ALLOCS.try_with(|n| n.set(n.get() + 1));
            let _ = BYTES.try_with(|n| n.set(n.get() + size));
        }

        unsafe impl GlobalAlloc for Counting {
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

        pub fn measure<R>(f: impl FnOnce() -> R) -> (R, Counts) {
            let (allocs, bytes) = (ALLOCS.get(), BYTES.get());
            let result = f();
            (result, Counts { allocs: ALLOCS.get() - allocs, bytes: BYTES.get() - bytes })
        }
    }

    #[global_allocator]
    static GLOBAL: counting::Counting = counting::Counting;

    use counting::measure;


    #[test]
    fn rendered_text_is_the_same() {
        let rows = [(1, "ann"), (22, "привет"), (333, "straße")];
        assert_eq!(render_lines(&rows), "1:ANN\n22:ПРИВЕТ\n333:STRASSE\n", "формат: id, двоеточие, имя в верхнем регистре, перевод строки");
        assert_eq!(render_lines(&[]), "");
    }

    #[test]
    fn rendering_does_not_allocate_per_row() {
        let names: Vec<String> = (0..1_000).map(|n| format!("user{n}")).collect();
        let rows: Vec<(u32, &str)> = names.iter().enumerate().map(|(i, n)| (i as u32, n.as_str())).collect();
        let (text, c) = measure(|| render_lines(&rows));
        assert_eq!(text.lines().count(), 1_000);
        assert!(c.allocs <= 4, "{} аллокаций на 1000 строк: на строку не должно быть ни одной, а ёмкость стоит выделить заранее", c.allocs);
    }

    #[test]
    fn squares_allocate_exactly_once() {
        let (all, c) = measure(|| squares(5_000));
        assert_eq!(all[70], 4_900);
        assert_eq!(all.len(), 5_000);
        assert_eq!(c.allocs, 1, "размер известен заранее: хватает одной аллокации, а не {} с расширениями", c.allocs);
    }

    #[test]
    fn encoder_reuses_its_buffer() {
        let messages: Vec<String> = (0..1_000).map(|n| format!("сообщение номер {n}")).collect();
        let (total, c) = measure(|| {
            let mut encoder = Encoder::new();
            let mut total = 0;
            for message in &messages {
                total += encoder.encode(message).len();
            }
            total
        });
        let expected: usize = messages.iter().map(|m| 5 + m.len()).sum();
        assert_eq!(total, expected, "в каждом кадре префикс MSG:, текст и перевод строки");
        assert!(c.allocs <= 6, "{} аллокаций на 1000 сообщений: буфер должен переиспользоваться (clear вместо нового Vec)", c.allocs);
    }

    #[test]
    fn encoder_output_is_correct() {
        let mut encoder = Encoder::new();
        assert_eq!(encoder.encode("hi"), b"MSG:hi\n");
        assert_eq!(encoder.encode("a"), b"MSG:a\n", "прежние данные не должны просачиваться");
    }
}
