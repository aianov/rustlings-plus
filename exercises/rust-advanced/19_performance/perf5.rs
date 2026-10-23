// В ленте три горячие функции, и все три копируют то, что можно было просто прочитать: слова упоминаний
// становятся новыми строками, тег переводится в нижний регистр даже когда он уже нижний, а у каждого поста
// свой экземпляр профиля автора со всеми строками внутри. На тысячах постов это тысячи лишних аллокаций.
// Убери копии там, где они не нужны, и оставь там, где без них не обойтись. Результаты не меняются.
use std::borrow::Cow;
use std::sync::Arc;

#[derive(Clone)]
pub struct Profile {
    pub name: String,
    pub bio: String,
}

pub struct Post {
    pub author: Profile,
    pub text: String,
}

pub fn mentions(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for word in text.split_whitespace() {
        if let Some(name) = word.strip_prefix('@') {
            let name = name.trim_end_matches(|c: char| !c.is_alphanumeric() && c != '_');
            if !name.is_empty() {
                found.push(name.to_string());
            }
        }
    }
    found
}

pub fn normalize_tag(tag: &str) -> String {
    tag.to_lowercase()
}

pub fn timeline(authors: &[Arc<Profile>], texts: &[&str]) -> Vec<Post> {
    let mut posts = Vec::new();
    for (i, text) in texts.iter().enumerate() {
        let author = (*authors[i % authors.len()]).clone();
        posts.push(Post { author, text: text.to_string() });
    }
    posts
}

fn main() {
    println!("{:?} {}", mentions("привет @ann"), normalize_tag("Rust"));
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
    fn mentions_are_found_without_copies() {
        let text = "привет @ann, и @bob! @ @_x и ещё e@mail";
        let (found, c) = measure(|| mentions(text));
        assert_eq!(found, ["ann", "bob", "_x"]);
        assert!(c.allocs <= 1, "{} аллокаций: имена можно брать срезами из исходной строки, нужна только память под сам вектор", c.allocs);
    }

    #[test]
    fn no_mentions_no_allocations() {
        let (found, c) = measure(|| mentions("просто текст без упоминаний"));
        assert!(found.is_empty());
        assert_eq!(c.allocs, 0, "пустой результат не должен ничего выделять");
    }

    #[test]
    fn tag_is_copied_only_when_it_changes() {
        let (tag, c) = measure(|| normalize_tag("rust-2024"));
        assert_eq!(&*tag, "rust-2024");
        assert_eq!(c.allocs, 0, "уже нормальный тег возвращается как есть, без копии");
        drop(tag);
        let (tag, c) = measure(|| normalize_tag("Rust-ÖL"));
        assert_eq!(&*tag, "rust-öl", "юникод тоже в нижний регистр");
        assert_eq!(c.allocs, 1, "изменённый тег это ровно одна новая строка");
    }

    #[test]
    fn profiles_are_shared_between_posts() {
        let authors: Vec<Arc<Profile>> = ["ann", "bob", "eve"]
            .iter()
            .map(|n| Arc::new(Profile { name: (*n).to_string(), bio: format!("о себе: {n}") }))
            .collect();
        let texts: Vec<&str> = std::iter::repeat_n("пост", 100).collect();
        let (posts, c) = measure(|| timeline(&authors, &texts));
        assert_eq!(posts.len(), 100);
        assert_eq!(posts[4].author.name, "bob", "авторы чередуются по кругу");
        assert_eq!(posts[4].text, "пост");
        assert!(
            c.allocs <= texts.len() + 2,
            "{} аллокаций на {} постов: профиль не нужно копировать в каждый пост, достаточно разделить его",
            c.allocs,
            texts.len()
        );
    }
}
