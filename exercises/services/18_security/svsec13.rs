// Сервис читает сообщение: сначала четыре байта длины, потом столько байт тела. Атакующий присылает длину 50 миллионов и ни одного байта тела. Сервис
// честно выделяет 50 МБ под тело, а таких соединений тысяча. Или присылает тело на 4 МБ: массив из миллиона тегов, и разбор ест память и процессор.
// Любое число из сети это недоверенный ввод. Лимит надо проверять до выделения памяти и до разбора, а не после.
// Цель: `read_body` отвергает объявленную длину сверх `MAX_BODY` не выделяя память под неё; `parse_post` отвергает слишком большое тело до разбора и
// проверяет форму: не больше `MAX_TAGS` тегов, каждый не длиннее `MAX_TAG_LEN` символов. Выделения памяти считает тест.


use serde::Deserialize;
use std::io::Read;

fn main() {}

pub const MAX_BODY: usize = 64 * 1024;
pub const MAX_TAGS: usize = 20;
pub const MAX_TAG_LEN: usize = 32;

#[derive(Debug, PartialEq, Eq)]
pub enum Reject {
    TooLarge,
    TooMany,
    TagTooLong,
    BadJson,
}

#[derive(Debug, Deserialize)]
pub struct Post {
    pub text: String,
    pub tags: Vec<String>,
}

/// Читает ровно `declared_len` байт тела.
pub fn read_body<R: Read>(reader: &mut R, declared_len: usize) -> Result<Vec<u8>, Reject> {
    let mut buf = vec![0u8; declared_len];
    reader.read_exact(&mut buf).map_err(|_| Reject::BadJson)?;
    Ok(buf)
}

pub fn parse_post(body: &[u8]) -> Result<Post, Reject> {
    serde_json::from_slice(body).map_err(|_| Reject::BadJson)
}

#[cfg(test)]
#[allow(unsafe_code)]
mod counting {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    thread_local! {
        static BYTES: Cell<u64> = const { Cell::new(0) };
    }

    pub struct Counting;

    unsafe impl GlobalAlloc for Counting {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            BYTES.with(|c| c.set(c.get() + layout.size() as u64));
            unsafe { System.alloc(layout) }
        }

        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            BYTES.with(|c| c.set(c.get() + layout.size() as u64));
            unsafe { System.alloc_zeroed(layout) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            unsafe { System.dealloc(ptr, layout) }
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            BYTES.with(|c| c.set(c.get() + new_size as u64));
            unsafe { System.realloc(ptr, layout, new_size) }
        }
    }

    #[global_allocator]
    static GLOBAL: Counting = Counting;

    /// Сколько байт запросила память внутри `f` (суммарно, в этом потоке).
    pub fn allocated<R>(f: impl FnOnce() -> R) -> (R, u64) {
        let before = BYTES.with(Cell::get);
        let result = f();
        (result, BYTES.with(Cell::get) - before)
    }
}

#[cfg(test)]
mod tests {
    use super::counting::allocated;
    use super::*;

    struct Endless;

    impl Read for Endless {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            buf.fill(b' ');
            Ok(buf.len())
        }
    }

    #[test]
    fn an_honest_message_is_read() {
        let body = r#"{"text":"привет","tags":["a","b"]}"#.as_bytes();
        let got = read_body(&mut &body[..], body.len()).unwrap();
        assert_eq!(got, body);
        let post = parse_post(&got).unwrap();
        assert_eq!((post.text.as_str(), post.tags.len()), ("привет", 2));
    }

    #[test]
    fn a_huge_declared_length_costs_no_memory() {
        let (r, bytes) = allocated(|| read_body(&mut Endless, 50_000_000));
        assert_eq!(r, Err(Reject::TooLarge));
        assert!(bytes < 64 * 1024, "на объявленные 50 МБ выделено {bytes} байт: память нельзя выделять до проверки длины");
        let (r, bytes) = allocated(|| read_body(&mut Endless, usize::MAX));
        assert_eq!(r, Err(Reject::TooLarge));
        assert!(bytes < 64 * 1024, "выделено {bytes} байт при длине usize::MAX");
    }

    #[test]
    fn the_limit_is_inclusive_and_short_bodies_are_errors() {
        assert_eq!(read_body(&mut Endless, MAX_BODY).map(|b| b.len()), Ok(MAX_BODY));
        assert_eq!(read_body(&mut Endless, MAX_BODY + 1), Err(Reject::TooLarge));
        let short = b"{}";
        assert_eq!(read_body(&mut &short[..], 100), Err(Reject::BadJson), "заявлено 100 байт, пришло 2");
        let long = [b' '; 10];
        assert_eq!(read_body(&mut &long[..], 4).unwrap().len(), 4, "лишнее после заявленной длины не читается");
    }

    #[test]
    fn a_million_tags_are_refused_before_parsing() {
        let mut body = String::from(r#"{"text":"x","tags":["#);
        for _ in 0..1_000_000 {
            body.push_str("\"a\",");
        }
        body.push_str("\"a\"]}");
        let (r, bytes) = allocated(|| parse_post(body.as_bytes()).map(|p| p.tags.len()));
        assert_eq!(r, Err(Reject::TooLarge));
        assert!(bytes < 64 * 1024, "разбор тела на 4 МБ выделил {bytes} байт: лимит размера должен срабатывать до разбора");
    }

    #[test]
    fn the_shape_is_limited_too() {
        let tags = |n: usize, len: usize| format!(r#"{{"text":"x","tags":[{}]}}"#, vec![format!("\"{}\"", "я".repeat(len)); n].join(","));
        assert!(parse_post(tags(20, 32).as_bytes()).is_ok(), "ровно предел проходит (32 символа это не 32 байта)");
        assert_eq!(parse_post(tags(21, 3).as_bytes()).unwrap_err(), Reject::TooMany);
        assert_eq!(parse_post(tags(2, 33).as_bytes()).unwrap_err(), Reject::TagTooLong);
        assert_eq!(parse_post(b"{").unwrap_err(), Reject::BadJson);
        let deep = format!("{}{}", "[".repeat(20_000), "]".repeat(20_000));
        assert_eq!(parse_post(deep.as_bytes()).unwrap_err(), Reject::BadJson, "глубокая вложенность это ошибка, а не падение стека");
    }
}
