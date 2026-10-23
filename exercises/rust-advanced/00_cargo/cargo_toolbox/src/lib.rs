// Заготовка библиотеки: сигнатуры и их смысл. Реализуй их по README.md.

pub const VERSION: &str = "TODO: строка из build.rs через env!";

#[derive(Debug, PartialEq, Eq)]
pub struct Report {
    pub lines: usize,
    pub words: usize,
    pub chars: usize,
}

pub fn stopwords() -> &'static [&'static str] {
    todo!("список стоп-слов, сгенерированный build.rs")
}

pub fn analyze(text: &str) -> Report {
    todo!("строки, слова (через крейт tokenizer) и символы")
}

pub fn top_words(text: &str, n: usize) -> Vec<(String, usize)> {
    todo!("n самых частых слов без стоп-слов: по убыванию частоты, при равенстве по алфавиту")
}

pub fn render_text(report: &Report) -> String {
    todo!("три строки: lines, words, chars")
}
