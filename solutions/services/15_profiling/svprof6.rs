
use serde::Deserialize;
use std::borrow::Cow;
use std::fmt::Write;

fn main() {}

#[derive(Deserialize)]
struct Post<'a> {
    user: &'a str,
    #[serde(borrow)]
    text: Cow<'a, str>,
    likes: u32,
}

pub fn handle(body: &str, out: &mut String) -> Result<(), String> {
    let post: Post<'_> = serde_json::from_str(body).map_err(|e| e.to_string())?;
    out.clear();
    out.extend(post.user.chars().flat_map(char::to_uppercase));
    write!(out, ": {} ({} likes)", post.text.trim(), post.likes).map_err(|e| e.to_string())
}

#[cfg(test)]
#[allow(unsafe_code)]
mod counting {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    thread_local! {
        static COUNT: Cell<u64> = const { Cell::new(0) };
    }

    pub struct Counting;

    unsafe impl GlobalAlloc for Counting {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            COUNT.with(|c| c.set(c.get() + 1));
            unsafe { System.alloc(layout) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            unsafe { System.dealloc(ptr, layout) }
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            COUNT.with(|c| c.set(c.get() + 1));
            unsafe { System.realloc(ptr, layout, new_size) }
        }
    }

    #[global_allocator]
    static GLOBAL: Counting = Counting;

    pub fn allocations<R>(f: impl FnOnce() -> R) -> (R, u64) {
        let before = COUNT.with(Cell::get);
        let result = f();
        (result, COUNT.with(Cell::get) - before)
    }
}

#[cfg(test)]
mod tests {
    use super::counting::allocations;
    use super::*;

    fn run(body: &str) -> Result<String, String> {
        let mut out = String::new();
        handle(body, &mut out).map(|()| out)
    }

    #[test]
    fn the_line_is_the_same() {
        assert_eq!(run(r#"{"user":"anna","text":"  привет, мир ","likes":12}"#).as_deref(), Ok("ANNA: привет, мир (12 likes)"));
        assert_eq!(run(r#"{"user":"ёж","text":"x","likes":0}"#).as_deref(), Ok("ЁЖ: x (0 likes)"), "заглавные буквы не только латинские");
        assert_eq!(run(r#"{"user":"straße","text":"x","likes":1}"#).as_deref(), Ok("STRASSE: x (1 likes)"), "ß при переводе в верхний регистр даёт две буквы");
    }

    #[test]
    fn escaped_text_still_works() {
        let got = run(r#"{"user":"bob","text":"строка\nвторая \"в кавычках\"","likes":3}"#);
        assert_eq!(got.as_deref(), Ok("BOB: строка\nвторая \"в кавычках\" (3 likes)"), "текст с экранированием нельзя взять заимствованным срезом: нужен вариант «заимствуй, если можно»");
    }

    #[test]
    fn bad_input_is_an_error_and_not_a_panic() {
        assert!(run("не json").is_err());
        assert!(run(r#"{"user":"a","text":"b"}"#).is_err(), "не хватает поля");
        assert!(run(r#"{"user":1,"text":"b","likes":2}"#).is_err(), "неверный тип");
    }

    #[test]
    fn the_buffer_is_replaced_not_appended_to() {
        let mut out = String::from("старое");
        handle(r#"{"user":"a","text":"b","likes":1}"#, &mut out).expect("разобрали");
        assert_eq!(out, "A: b (1 likes)");
        handle(r#"{"user":"c","text":"d","likes":2}"#, &mut out).expect("разобрали");
        assert_eq!(out, "C: d (2 likes)");
    }

    #[test]
    fn in_steady_state_the_allocator_is_nearly_idle() {
        let body = r#"{"user":"anna","text":"привет, мир, это обычный пост средней длины","likes":12}"#;
        let mut out = String::with_capacity(256);
        handle(body, &mut out).expect("разогрев");
        let ((), allocs) = allocations(|| {
            for _ in 0..200 {
                handle(body, &mut out).expect("разобрали");
            }
        });
        assert!(allocs <= 20, "на 200 запросов {allocs} выделений: на каждый запрос должно быть не больше одного, а лучше ни одного (временные String, to_uppercase и format! дают десяток на запрос)");
    }

    #[test]
    fn the_counter_itself_works() {
        let (_, none) = allocations(|| 1 + 1);
        let (_, some) = allocations(|| std::hint::black_box(vec![1u8; 100]));
        assert_eq!(none, 0);
        assert!(some >= 1, "счётчик должен видеть выделения, иначе проверка выше ничего не значит");
    }
}
