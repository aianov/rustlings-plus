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

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn matches(pattern: &str, text: &str) -> bool {
        Pattern::parse(pattern).expect("шаблон должен разбираться").is_match(text)
    }

    #[test]
    fn username_pattern() {
        let p = "[a-z][a-z0-9_]{2,19}";
        assert!(matches(p, "alice"), "alice подходит");
        assert!(matches(p, "bob_42"), "bob_42 подходит");
        assert!(!matches(p, "ab"), "слишком короткий");
        assert!(!matches(p, "1abc"), "начинается с цифры");
        assert!(!matches(p, &format!("a{}", "b".repeat(20))), "21 символ");
        assert!(matches(p, &format!("a{}", "b".repeat(19))), "20 символов");
    }

    #[test]
    fn pattern_must_match_the_whole_string() {
        assert!(matches("abc", "abc"), "точное совпадение");
        assert!(!matches("abc", "abcd"), "хвост лишний");
        assert!(!matches("abc", "xabc"), "голова лишняя");
        assert!(matches("", ""), "пустой шаблон подходит пустой строке");
        assert!(!matches("", "a"), "но не непустой");
    }

    #[test]
    fn quantifiers() {
        assert!(matches("colou?r", "color") && matches("colou?r", "colour"), "? значит ноль или один");
        assert!(matches("ab*c", "ac") && matches("ab*c", "abbbc"), "* значит сколько угодно");
        assert!(!matches("ab+c", "ac") && matches("ab+c", "abc"), "+ значит хотя бы один");
        assert!(matches("a{3}", "aaa") && !matches("a{3}", "aa") && !matches("a{3}", "aaaa"), "{{3}} ровно три");
        assert!(matches("a{2,}", "aa") && matches("a{2,}", "aaaaa") && !matches("a{2,}", "a"), "{{2,}} значит от двух и больше");
        assert!(matches("a{2,3}", "aa") && matches("a{2,3}", "aaa") && !matches("a{2,3}", "aaaa"), "{{2,3}} от двух до трёх");
    }

    #[test]
    fn greedy_parts_give_way_to_the_tail() {
        assert!(matches("a*ab", "aaab"), "a* должна отдать одну букву, чтобы хватило на ab");
        assert!(matches(".*@.*\\.com", "alice@example.com"), "точка и звёздочка, потом литерал");
        assert!(!matches(".*@.*\\.com", "alice@example.org"), "домен не тот");
    }

    #[test]
    fn classes_ranges_and_negation() {
        assert!(matches("[a-c]+", "abcabc") && !matches("[a-c]+", "abd"), "диапазон");
        assert!(matches("[^ ]+", "no_spaces") && !matches("[^ ]+", "has space"), "отрицание");
        assert!(matches("[а-я]+", "привет") && !matches("[а-я]+", "hello"), "кириллица по символам");
        assert!(matches("[a-z-]+", "a-b-c"), "дефис в конце класса это обычный символ");
        assert!(matches("[\\]a]+", "a]a"), "экранированная скобка внутри класса");
    }

    #[test]
    fn escapes_make_special_characters_literal() {
        assert!(matches("a\\.b", "a.b") && !matches("a\\.b", "axb"), "\\. это точка, а не любой символ");
        assert!(matches("\\[x\\]", "[x]"), "квадратные скобки");
        assert!(matches("1\\+1", "1+1"), "плюс");
        assert!(matches("a.c", "a€c") && matches("a.c", "aяc"), "точка не ограничена ASCII");
    }

    #[test]
    fn broken_patterns_report_what_is_wrong() {
        assert_eq!(Pattern::parse("[abc").unwrap_err(), PatternError::Unclosed, "нет закрывающей ]");
        assert_eq!(Pattern::parse("[]").unwrap_err(), PatternError::EmptyClass, "пустой класс");
        assert_eq!(Pattern::parse("[z-a]").unwrap_err(), PatternError::BadRange, "диапазон наоборот");
        assert_eq!(Pattern::parse("*a").unwrap_err(), PatternError::NothingToRepeat, "повтор без чего повторять");
        assert_eq!(Pattern::parse("a**").unwrap_err(), PatternError::NothingToRepeat, "повтор повтора");
        assert_eq!(Pattern::parse("a{3,2}").unwrap_err(), PatternError::BadQuantifier, "минимум больше максимума");
        assert_eq!(Pattern::parse("a{x}").unwrap_err(), PatternError::BadQuantifier, "не число в скобках");
        assert_eq!(Pattern::parse("a{2").unwrap_err(), PatternError::BadQuantifier, "нет закрывающей }}");
        assert_eq!(Pattern::parse("ab\\").unwrap_err(), PatternError::BadEscape, "обратная косая в конце");
    }

    #[test]
    fn many_stars_do_not_explode() {
        let pattern = Pattern::parse("a*a*a*a*a*a*a*a*b").unwrap();
        let text = "a".repeat(60);
        let started = std::time::Instant::now();
        assert!(!pattern.is_match(&text), "b в строке нет");
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "перебор всех вариантов слишком долгий: запоминай состояния (номер элемента, позиция), из которых совпасть не вышло"
        );
    }
}
