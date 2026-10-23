use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::num::ParseIntError;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "json", derive(serde::Serialize), serde(rename_all = "UPPERCASE"))]
pub enum Level {
    Debug,
    Info,
    Warn,
    Error,
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Level::Debug => "DEBUG",
            Level::Info => "INFO",
            Level::Warn => "WARN",
            Level::Error => "ERROR",
        })
    }
}

impl FromStr for Level {
    type Err = ParseError;

    fn from_str(text: &str) -> Result<Level, ParseError> {
        match text {
            "DEBUG" => Ok(Level::Debug),
            "INFO" => Ok(Level::Info),
            "WARN" => Ok(Level::Warn),
            "ERROR" => Ok(Level::Error),
            other => Err(ParseError::UnknownLevel(other.to_string())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LevelSet(u8);

impl LevelSet {
    pub const fn empty() -> LevelSet {
        LevelSet(0)
    }

    pub const fn with(self, level: Level) -> LevelSet {
        LevelSet(self.0 | 1 << level as u8)
    }

    pub const fn contains(self, level: Level) -> bool {
        self.0 & 1 << level as u8 != 0
    }

    pub const fn len(self) -> usize {
        self.0.count_ones() as usize
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

#[macro_export]
macro_rules! level_set {
    ($($level:ident),* $(,)?) => {
        $crate::LevelSet::empty() $(.with($crate::Level::$level))*
    };
}

#[derive(Debug, PartialEq)]
pub enum ParseError {
    MissingField(&'static str),
    BadTimestamp(ParseIntError),
    UnknownLevel(String),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::MissingField(name) => write!(f, "нет поля {name}"),
            ParseError::BadTimestamp(error) => write!(f, "плохая метка времени: {error}"),
            ParseError::UnknownLevel(level) => write!(f, "неизвестный уровень: {level}"),
        }
    }
}

impl std::error::Error for ParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ParseError::BadTimestamp(error) => Some(error),
            _ => None,
        }
    }
}

impl From<ParseIntError> for ParseError {
    fn from(error: ParseIntError) -> ParseError {
        ParseError::BadTimestamp(error)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entry<'a> {
    pub ts: u64,
    pub level: Level,
    pub target: &'a str,
    pub message: &'a str,
}

/// Разбирает одну строку журнала вида `1700000003 WARN db медленный запрос`.
///
/// ```
/// use logq::{parse_line, Level};
///
/// let entry = parse_line("1700000003 WARN db медленный запрос").unwrap();
/// assert_eq!((entry.ts, entry.level, entry.target), (1_700_000_003, Level::Warn, "db"));
/// assert_eq!(entry.message, "медленный запрос");
/// ```
pub fn parse_line(line: &str) -> Result<Entry<'_>, ParseError> {
    let mut parts = line.splitn(4, ' ');
    let ts = parts.next().filter(|part| !part.is_empty()).ok_or(ParseError::MissingField("timestamp"))?.parse::<u64>()?;
    let level = parts.next().ok_or(ParseError::MissingField("level"))?.parse::<Level>()?;
    let target = parts.next().filter(|part| !part.is_empty()).ok_or(ParseError::MissingField("target"))?;
    let message = parts.next().filter(|part| !part.is_empty()).ok_or(ParseError::MissingField("message"))?;
    Ok(Entry { ts, level, target, message })
}

#[derive(Debug, PartialEq)]
pub struct LineError {
    pub line: usize,
    pub error: ParseError,
}

/// Лениво разбирает журнал построчно, пропуская пустые строки.
///
/// ```
/// let text = "1 INFO a x\n\nbad\n2 ERROR b y";
/// let bad: Vec<usize> = logq::entries(text).filter_map(|item| item.err()).map(|e| e.line).collect();
/// assert_eq!(bad, vec![3]);
/// ```
pub fn entries(text: &str) -> impl Iterator<Item = Result<Entry<'_>, LineError>> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(index, line)| parse_line(line).map_err(|error| LineError { line: index + 1, error }))
}

pub trait Aggregate {
    type Out;

    fn add(&mut self, entry: &Entry<'_>);

    fn finish(self) -> Self::Out;
}

pub fn run<'a, A: Aggregate>(entries: impl IntoIterator<Item = Entry<'a>>, mut aggregate: A) -> A::Out {
    for entry in entries {
        aggregate.add(&entry);
    }
    aggregate.finish()
}

pub fn only<'a>(entries: impl Iterator<Item = Entry<'a>>, levels: LevelSet) -> impl Iterator<Item = Entry<'a>> {
    entries.filter(move |entry| levels.contains(entry.level))
}

#[derive(Default)]
pub struct CountByLevel {
    counts: BTreeMap<Level, usize>,
}

impl Aggregate for CountByLevel {
    type Out = BTreeMap<Level, usize>;

    fn add(&mut self, entry: &Entry<'_>) {
        *self.counts.entry(entry.level).or_default() += 1;
    }

    fn finish(self) -> Self::Out {
        self.counts
    }
}

pub struct TopTargets {
    limit: usize,
    counts: HashMap<String, usize>,
}

impl TopTargets {
    pub fn new(limit: usize) -> TopTargets {
        TopTargets { limit, counts: HashMap::new() }
    }
}

impl Aggregate for TopTargets {
    type Out = Vec<(String, usize)>;

    fn add(&mut self, entry: &Entry<'_>) {
        match self.counts.get_mut(entry.target) {
            Some(count) => *count += 1,
            None => {
                self.counts.insert(entry.target.to_string(), 1);
            }
        }
    }

    fn finish(self) -> Self::Out {
        let mut all: Vec<(String, usize)> = self.counts.into_iter().collect();
        all.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        all.truncate(self.limit);
        all
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "json", derive(serde::Serialize), serde(rename_all = "camelCase"))]
pub struct Report {
    pub total: usize,
    pub bad_lines: usize,
    pub by_level: BTreeMap<Level, usize>,
}

impl Report {
    pub fn merge(&mut self, other: Report) {
        self.total += other.total;
        self.bad_lines += other.bad_lines;
        for (level, count) in other.by_level {
            *self.by_level.entry(level).or_default() += count;
        }
    }

    #[cfg(feature = "json")]
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("отчёт всегда сериализуется")
    }
}

pub fn summarize(text: &str) -> Report {
    let mut report = Report::default();
    for item in entries(text) {
        match item {
            Ok(entry) => {
                report.total += 1;
                *report.by_level.entry(entry.level).or_default() += 1;
            }
            Err(_) => report.bad_lines += 1,
        }
    }
    report
}

fn split_by_lines(text: &str, parts: usize) -> Vec<&str> {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    if lines.is_empty() {
        return Vec::new();
    }
    let parts = parts.clamp(1, lines.len());
    let (base, extra) = (lines.len() / parts, lines.len() % parts);
    let mut iter = lines.into_iter();
    let mut rest = text;
    let mut chunks = Vec::with_capacity(parts);
    for index in 0..parts {
        let take = base + usize::from(index < extra);
        let len: usize = (&mut iter).take(take).map(str::len).sum();
        let (chunk, tail) = rest.split_at(len);
        chunks.push(chunk);
        rest = tail;
    }
    chunks
}

pub fn par_map_chunks<R: Send>(text: &str, workers: usize, f: impl Fn(&str) -> R + Sync) -> Vec<R> {
    let chunks = split_by_lines(text, workers);
    std::thread::scope(|scope| {
        let f = &f;
        let handles: Vec<_> = chunks.into_iter().map(|chunk| scope.spawn(move || f(chunk))).collect();
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap_or_else(|payload| std::panic::resume_unwind(payload)))
            .collect()
    })
}

pub fn summarize_parallel(text: &str, workers: usize) -> Report {
    par_map_chunks(text, workers, summarize).into_iter().fold(Report::default(), |mut total, part| {
        total.merge(part);
        total
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_cover_the_text_without_cutting_lines() {
        let text = "a\nb\nc\nd\ne\n";
        let chunks = split_by_lines(text, 2);
        assert_eq!(chunks, vec!["a\nb\nc\n", "d\ne\n"]);
        assert_eq!(chunks.concat(), text);
    }

    #[test]
    fn more_workers_than_lines_gives_one_line_each() {
        assert_eq!(split_by_lines("x\ny", 10), vec!["x\n", "y"]);
    }

    #[test]
    fn empty_text_has_no_chunks() {
        assert!(split_by_lines("", 4).is_empty());
    }

    #[test]
    fn level_set_bits_do_not_collide() {
        let all = LevelSet::empty().with(Level::Debug).with(Level::Info).with(Level::Warn).with(Level::Error);
        assert_eq!(all.len(), 4);
    }
}
