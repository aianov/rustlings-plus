include!(concat!(env!("OUT_DIR"), "/words.rs"));

pub const BUILD: &str = env!("WORDLIST_BUILD");

pub fn contains(word: &str) -> bool {
    WORDS.binary_search(&word).is_ok()
}
