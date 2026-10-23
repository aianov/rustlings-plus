use std::collections::HashSet;

#[derive(Debug, PartialEq)]
pub enum PatternError {
    Unclosed,
    EmptyClass,
    BadRange,
    BadEscape,
    NothingToRepeat,
    BadQuantifier,
}

#[derive(Debug)]
enum Atom {
    Char(char),
    Any,
    Class { negated: bool, ranges: Vec<(char, char)> },
}

#[derive(Debug)]
struct Item {
    atom: Atom,
    min: usize,
    max: usize,
}

#[derive(Debug)]
pub struct Pattern {
    items: Vec<Item>,
}

impl Atom {
    fn matches(&self, c: char) -> bool {
        match self {
            Atom::Char(expected) => *expected == c,
            Atom::Any => true,
            Atom::Class { negated, ranges } => ranges.iter().any(|(lo, hi)| (*lo..=*hi).contains(&c)) != *negated,
        }
    }
}

impl Pattern {
    pub fn parse(src: &str) -> Result<Pattern, PatternError> {
        let chars: Vec<char> = src.chars().collect();
        let mut items: Vec<Item> = Vec::new();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            i += 1;
            let (min, max) = match c {
                '?' | '*' | '+' | '{' => {
                    let Some(last) = items.last_mut() else {
                        return Err(PatternError::NothingToRepeat);
                    };
                    if last.min != 1 || last.max != 1 {
                        return Err(PatternError::NothingToRepeat);
                    }
                    let bounds = match c {
                        '?' => (0, 1),
                        '*' => (0, usize::MAX),
                        '+' => (1, usize::MAX),
                        _ => {
                            let close = chars[i..].iter().position(|&x| x == '}').ok_or(PatternError::BadQuantifier)?;
                            let body: String = chars[i..i + close].iter().collect();
                            i += close + 1;
                            let number = |s: &str| s.parse::<usize>().map_err(|_| PatternError::BadQuantifier);
                            match body.split_once(',') {
                                None => {
                                    let n = number(&body)?;
                                    (n, n)
                                }
                                Some((lo, "")) => (number(lo)?, usize::MAX),
                                Some((lo, hi)) => {
                                    let (lo, hi) = (number(lo)?, number(hi)?);
                                    if lo > hi {
                                        return Err(PatternError::BadQuantifier);
                                    }
                                    (lo, hi)
                                }
                            }
                        }
                    };
                    last.min = bounds.0;
                    last.max = bounds.1;
                    continue;
                }
                _ => (1, 1),
            };
            let atom = match c {
                '.' => Atom::Any,
                '\\' => {
                    let escaped = *chars.get(i).ok_or(PatternError::BadEscape)?;
                    i += 1;
                    Atom::Char(escaped)
                }
                '[' => {
                    let negated = chars.get(i) == Some(&'^');
                    if negated {
                        i += 1;
                    }
                    let mut ranges = Vec::new();
                    loop {
                        let Some(&c) = chars.get(i) else {
                            return Err(PatternError::Unclosed);
                        };
                        i += 1;
                        if c == ']' {
                            break;
                        }
                        let lo = if c == '\\' {
                            let escaped = *chars.get(i).ok_or(PatternError::Unclosed)?;
                            i += 1;
                            escaped
                        } else {
                            c
                        };
                        if chars.get(i) == Some(&'-') && chars.get(i + 1).is_some_and(|&n| n != ']') {
                            let hi = chars[i + 1];
                            i += 2;
                            if hi < lo {
                                return Err(PatternError::BadRange);
                            }
                            ranges.push((lo, hi));
                        } else {
                            ranges.push((lo, lo));
                        }
                    }
                    if ranges.is_empty() {
                        return Err(PatternError::EmptyClass);
                    }
                    Atom::Class { negated, ranges }
                }
                other => Atom::Char(other),
            };
            items.push(Item { atom, min, max });
        }
        Ok(Pattern { items })
    }

    pub fn is_match(&self, text: &str) -> bool {
        let chars: Vec<char> = text.chars().collect();
        let mut failed = HashSet::new();
        self.matches_from(0, 0, &chars, &mut failed)
    }

    fn matches_from(&self, item: usize, pos: usize, chars: &[char], failed: &mut HashSet<(usize, usize)>) -> bool {
        let Some(current) = self.items.get(item) else {
            return pos == chars.len();
        };
        if failed.contains(&(item, pos)) {
            return false;
        }
        let mut available = 0;
        while available < current.max && pos + available < chars.len() && current.atom.matches(chars[pos + available]) {
            available += 1;
        }
        if available >= current.min {
            for taken in (current.min..=available).rev() {
                if self.matches_from(item + 1, pos + taken, chars, failed) {
                    return true;
                }
            }
        }
        failed.insert((item, pos));
        false
    }
}
