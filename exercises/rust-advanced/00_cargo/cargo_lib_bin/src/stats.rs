#[derive(Debug, PartialEq, Eq)]
pub struct Stats {
    pub lines: usize,
    pub words: usize,
    pub bytes: usize,
}

pub fn analyze(text: &str) -> Stats {
    Stats {
        lines: text.lines().count(),
        words: text.split_whitespace().count(),
        bytes: text.len(),
    }
}
