use std::collections::BTreeMap;

include!(concat!(env!("OUT_DIR"), "/stopwords.rs"));

pub const VERSION: &str = env!("TEXTSTAT_VERSION");

#[derive(Debug, PartialEq, Eq)]
pub struct Report {
    pub lines: usize,
    pub words: usize,
    pub chars: usize,
}

pub fn stopwords() -> &'static [&'static str] {
    STOPWORDS
}

pub fn analyze(text: &str) -> Report {
    Report {
        lines: text.lines().count(),
        words: tokenizer::words(text).count(),
        chars: text.chars().count(),
    }
}

pub fn top_words(text: &str, n: usize) -> Vec<(String, usize)> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for word in tokenizer::words(text) {
        if STOPWORDS.binary_search(&word.as_str()).is_err() {
            *counts.entry(word).or_default() += 1;
        }
    }
    let mut list: Vec<(String, usize)> = counts.into_iter().collect();
    list.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    list.truncate(n);
    list
}

pub fn render_text(report: &Report) -> String {
    format!("lines: {}\nwords: {}\nchars: {}", report.lines, report.words, report.chars)
}

#[cfg(feature = "json")]
pub fn render_json(report: &Report) -> String {
    format!(
        "{{\"lines\":{},\"words\":{},\"chars\":{}}}",
        report.lines, report.words, report.chars
    )
}
