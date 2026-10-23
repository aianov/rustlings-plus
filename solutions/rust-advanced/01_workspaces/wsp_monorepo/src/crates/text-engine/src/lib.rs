mod lexer;
mod mentions;

pub use mentions::extract as mentions;

pub fn tokenize(text: &str) -> Vec<String> {
    lexer::split_words(text)
}
