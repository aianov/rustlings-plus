#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl Version {
    pub fn parse(s: &str) -> Option<Version> {
        let number = |part: &str| -> Option<u64> {
            if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            part.parse().ok()
        };
        let mut parts = s.split('.');
        let major = number(parts.next()?)?;
        let minor = number(parts.next()?)?;
        let patch = number(parts.next()?)?;
        if parts.next().is_some() {
            return None;
        }
        Some(Version { major, minor, patch })
    }
}

pub fn matches(req: &str, version: &str) -> bool {
    let Some(version) = Version::parse(version) else {
        return false;
    };
    if let Some(exact) = req.strip_prefix('=') {
        return Version::parse(exact) == Some(version);
    }
    let base = req.strip_prefix('^').unwrap_or(req);
    let Some(low) = Version::parse(base) else {
        return false;
    };
    version >= low && version.major == low.major
}

fn main() {
    println!("{:?}", Version::parse("1.2.3"));
    println!("{}", matches("^1.2.3", "1.4.0"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_accepts_three_numbers() {
        assert_eq!(
            Version::parse("1.22.333"),
            Some(Version { major: 1, minor: 22, patch: 333 }),
            "`1.22.333` это major 1, minor 22, patch 333"
        );
    }

    #[test]
    fn parse_rejects_garbage() {
        for bad in ["", "1", "1.2", "1.2.3.4", "a.b.c", "1.2.x", "1..3", "-1.2.3", "+1.2.3", " 1.2.3"] {
            assert_eq!(Version::parse(bad), None, "`{bad}` не версия из трёх чисел, а parse её принял");
        }
    }

    #[test]
    fn versions_compare_by_number_not_by_text() {
        let a = Version::parse("1.9.0").unwrap();
        let b = Version::parse("1.10.0").unwrap();
        assert!(a < b, "1.10.0 новее 1.9.0: сравнивай числа, а не строки");
    }

    #[test]
    fn caret_allows_newer_within_same_major() {
        for v in ["1.2.3", "1.2.4", "1.3.0", "1.99.7"] {
            assert!(matches("^1.2.3", v), "^1.2.3 должна пускать {v}");
        }
    }

    #[test]
    fn caret_rejects_older_and_next_major() {
        for v in ["1.2.2", "1.1.9", "2.0.0", "0.9.0"] {
            assert!(!matches("^1.2.3", v), "^1.2.3 не должна пускать {v}");
        }
    }

    #[test]
    fn bare_requirement_means_caret() {
        assert!(matches("1.2.3", "1.8.0"), "`1.2.3` без знака это то же, что `^1.2.3`");
        assert!(!matches("1.2.3", "2.0.0"), "`1.2.3` без знака не должна пускать следующий major");
    }

    #[test]
    fn equals_means_exactly_one_version() {
        assert!(matches("=1.2.3", "1.2.3"), "=1.2.3 пускает 1.2.3");
        assert!(!matches("=1.2.3", "1.2.4"), "=1.2.3 не пускает 1.2.4");
        assert!(!matches("=1.2.3", "1.2.2"), "=1.2.3 не пускает 1.2.2");
    }

    #[test]
    fn garbage_matches_nothing() {
        assert!(!matches("^1.2.3", "banana"), "версия `banana` не должна подходить ни под что");
        assert!(!matches("^banana", "1.2.3"), "требование `^banana` не должно пускать ничего");
        assert!(!matches("", "1.2.3"), "пустое требование не должно пускать ничего");
    }
}
