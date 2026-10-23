use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternError {
    Unclosed,
    EmptyClass,
    BadRange,
    BadEscape,
    NothingToRepeat,
    BadQuantifier,
}

#[derive(Debug, Clone)]
enum Atom {
    Char(char),
    Any,
    Class {
        negated: bool,
        ranges: Vec<(char, char)>,
    },
}

#[derive(Debug, Clone)]
struct Item {
    atom: Atom,
    min: usize,
    max: usize,
}

#[derive(Debug, Clone)]
pub struct Pattern {
    items: Vec<Item>,
}

impl Atom {
    fn matches(&self, c: char) -> bool {
        match self {
            Atom::Char(expected) => *expected == c,
            Atom::Any => true,
            Atom::Class { negated, ranges } => {
                ranges.iter().any(|(lo, hi)| (*lo..=*hi).contains(&c)) != *negated
            }
        }
    }
}

fn quantifier(chars: &[char], at: &mut usize) -> Result<(usize, usize), PatternError> {
    let close = chars[*at..]
        .iter()
        .position(|&c| c == '}')
        .ok_or(PatternError::BadQuantifier)?;
    let body: String = chars[*at..*at + close].iter().collect();
    *at += close + 1;
    let number = |text: &str| {
        text.parse::<usize>()
            .map_err(|_| PatternError::BadQuantifier)
    };
    match body.split_once(',') {
        None => {
            let n = number(&body)?;
            Ok((n, n))
        }
        Some((lo, "")) => Ok((number(lo)?, usize::MAX)),
        Some((lo, hi)) => {
            let (lo, hi) = (number(lo)?, number(hi)?);
            if lo > hi {
                return Err(PatternError::BadQuantifier);
            }
            Ok((lo, hi))
        }
    }
}

fn class(chars: &[char], at: &mut usize) -> Result<Atom, PatternError> {
    let negated = chars.get(*at) == Some(&'^');
    if negated {
        *at += 1;
    }
    let mut ranges = Vec::new();
    loop {
        let Some(&c) = chars.get(*at) else {
            return Err(PatternError::Unclosed);
        };
        *at += 1;
        if c == ']' {
            break;
        }
        let lo = if c == '\\' {
            let escaped = *chars.get(*at).ok_or(PatternError::Unclosed)?;
            *at += 1;
            escaped
        } else {
            c
        };
        if chars.get(*at) == Some(&'-') && chars.get(*at + 1).is_some_and(|&n| n != ']') {
            let hi = chars[*at + 1];
            *at += 2;
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
    Ok(Atom::Class { negated, ranges })
}

impl Pattern {
    pub fn parse(source: &str) -> Result<Pattern, PatternError> {
        let chars: Vec<char> = source.chars().collect();
        let mut items: Vec<Item> = Vec::new();
        let mut at = 0;
        while at < chars.len() {
            let c = chars[at];
            at += 1;
            if matches!(c, '?' | '*' | '+' | '{') {
                let Some(last) = items.last_mut() else {
                    return Err(PatternError::NothingToRepeat);
                };
                if last.min != 1 || last.max != 1 {
                    return Err(PatternError::NothingToRepeat);
                }
                let (min, max) = match c {
                    '?' => (0, 1),
                    '*' => (0, usize::MAX),
                    '+' => (1, usize::MAX),
                    _ => quantifier(&chars, &mut at)?,
                };
                last.min = min;
                last.max = max;
                continue;
            }
            let atom = match c {
                '.' => Atom::Any,
                '\\' => {
                    let escaped = *chars.get(at).ok_or(PatternError::BadEscape)?;
                    at += 1;
                    Atom::Char(escaped)
                }
                '[' => class(&chars, &mut at)?,
                other => Atom::Char(other),
            };
            items.push(Item {
                atom,
                min: 1,
                max: 1,
            });
        }
        Ok(Pattern { items })
    }

    fn close(&self, states: &mut Vec<(usize, usize)>, seen: &mut HashSet<(usize, usize)>) {
        let mut at = 0;
        while at < states.len() {
            let (item, count) = states[at];
            at += 1;
            let Some(current) = self.items.get(item) else {
                continue;
            };
            if count >= current.min && seen.insert((item + 1, 0)) {
                states.push((item + 1, 0));
            }
        }
    }

    pub fn is_match(&self, text: &str) -> bool {
        let mut seen = HashSet::new();
        let mut states = vec![(0, 0)];
        seen.insert((0, 0));
        self.close(&mut states, &mut seen);
        for c in text.chars() {
            let mut next = Vec::new();
            let mut fresh = HashSet::new();
            for &(item, count) in &states {
                let Some(current) = self.items.get(item) else {
                    continue;
                };
                if count >= current.max || !current.atom.matches(c) {
                    continue;
                }
                let counted = if current.max == usize::MAX {
                    (count + 1).min(current.min.max(1))
                } else {
                    count + 1
                };
                if fresh.insert((item, counted)) {
                    next.push((item, counted));
                }
            }
            self.close(&mut next, &mut fresh);
            if next.is_empty() {
                return false;
            }
            states = next;
        }
        states.iter().any(|&(item, _)| item == self.items.len())
    }
}
