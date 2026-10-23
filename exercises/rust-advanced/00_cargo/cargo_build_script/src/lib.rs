// TODO: подключи сгенерированный words.rs из OUT_DIR и объяви константу BUILD из переменной окружения
// WORDLIST_BUILD, которую выставляет build.rs.

pub fn contains(word: &str) -> bool {
    WORDS.binary_search(&word).is_ok()
}
