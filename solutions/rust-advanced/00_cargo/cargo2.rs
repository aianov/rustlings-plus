#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl Version {
    pub fn new(major: u64, minor: u64, patch: u64) -> Version {
        Version { major, minor, patch }
    }

    pub fn parse(s: &str) -> Option<Version> {
        let mut parts = s.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next()?.parse().ok()?;
        let patch = parts.next()?.parse().ok()?;
        if parts.next().is_some() {
            return None;
        }
        Some(Version { major, minor, patch })
    }
}

#[derive(Clone, Copy)]
enum Op {
    Caret,
    Tilde,
    Exact,
}

struct Parts {
    major: u64,
    minor: Option<u64>,
    patch: Option<u64>,
}

fn parse_parts(s: &str) -> Option<Parts> {
    let mut it = s.split('.');
    let major = it.next()?.parse().ok()?;
    let minor = match it.next() {
        Some(m) => Some(m.parse().ok()?),
        None => None,
    };
    let patch = match it.next() {
        Some(p) => Some(p.parse().ok()?),
        None => None,
    };
    if it.next().is_some() {
        return None;
    }
    Some(Parts { major, minor, patch })
}

#[derive(Debug)]
pub struct Req {
    lower: Version,
    upper: Option<Version>,
}

impl Req {
    pub fn parse(s: &str) -> Option<Req> {
        if s == "*" {
            return Some(Req { lower: Version::new(0, 0, 0), upper: None });
        }
        let (op, rest) = match s.chars().next()? {
            '^' => (Op::Caret, &s[1..]),
            '~' => (Op::Tilde, &s[1..]),
            '=' => (Op::Exact, &s[1..]),
            _ => match s.strip_suffix(".*") {
                Some(prefix) => (Op::Tilde, prefix),
                None => (Op::Caret, s),
            },
        };
        let parts = parse_parts(rest)?;
        if matches!(op, Op::Exact) && parts.patch.is_none() {
            return None;
        }
        let lower = Version::new(parts.major, parts.minor.unwrap_or(0), parts.patch.unwrap_or(0));
        Some(Req { lower, upper: Some(upper_bound(op, &parts)) })
    }

    pub fn matches(&self, v: &Version) -> bool {
        *v >= self.lower && self.upper.is_none_or(|u| *v < u)
    }
}

pub fn matches(req: &str, version: &str) -> bool {
    match (Req::parse(req), Version::parse(version)) {
        (Some(req), Some(version)) => req.matches(&version),
        _ => false,
    }
}
fn upper_bound(op: Op, p: &Parts) -> Version {
    match op {
        Op::Exact => Version::new(p.major, p.minor.unwrap_or(0), p.patch.unwrap_or(0) + 1),
        Op::Tilde => match p.minor {
            Some(minor) => Version::new(p.major, minor + 1, 0),
            None => Version::new(p.major + 1, 0, 0),
        },
        Op::Caret => match (p.major, p.minor, p.patch) {
            (major, _, _) if major > 0 => Version::new(major + 1, 0, 0),
            (_, None, _) => Version::new(1, 0, 0),
            (_, Some(minor), _) if minor > 0 => Version::new(0, minor + 1, 0),
            (_, Some(_), None) => Version::new(0, 1, 0),
            (_, Some(_), Some(patch)) => Version::new(0, 0, patch + 1),
        },
    }
}

fn main() {
    println!("{}", matches("^0.2.3", "0.2.9"));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(req: &str, version: &str, expected: bool) {
        let verdict = if expected { "должна пускать" } else { "не должна пускать" };
        assert_eq!(matches(req, version), expected, "{req} {verdict} {version}");
    }

    #[test]
    fn caret_after_nonzero_major() {
        check("^1.2.3", "1.9.9", true);
        check("^1.2.3", "1.2.2", false);
        check("^1.2.3", "2.0.0", false);
        check("^1.2", "1.9.0", true);
        check("^1", "1.0.1", true);
        check("^1", "2.0.0", false);
    }

    #[test]
    fn caret_zero_major_pins_minor() {
        check("^0.2.3", "0.2.3", true);
        check("^0.2.3", "0.2.9", true);
        check("^0.2.3", "0.3.0", false);
        check("^0.2.3", "0.2.2", false);
        check("^0.2", "0.2.5", true);
        check("^0.2", "0.3.0", false);
    }

    #[test]
    fn caret_zero_zero_pins_patch() {
        check("^0.0.3", "0.0.3", true);
        check("^0.0.3", "0.0.4", false);
        check("^0.0", "0.0.9", true);
        check("^0.0", "0.1.0", false);
        check("^0", "0.9.9", true);
        check("^0", "1.0.0", false);
    }

    #[test]
    fn bare_requirement_is_caret() {
        check("0.2.3", "0.2.9", true);
        check("0.2.3", "0.3.0", false);
        check("1.2.3", "1.9.0", true);
        check("1.2.3", "2.0.0", false);
    }

    #[test]
    fn tilde_keeps_minor() {
        check("~1.2.3", "1.2.3", true);
        check("~1.2.3", "1.2.9", true);
        check("~1.2.3", "1.3.0", false);
        check("~1.2", "1.2.7", true);
        check("~1.2", "1.3.0", false);
        check("~1", "1.9.0", true);
        check("~1", "2.0.0", false);
        check("~1.2.3", "1.2.2", false);
    }

    #[test]
    fn exact_is_one_version() {
        check("=1.2.3", "1.2.3", true);
        check("=1.2.3", "1.2.4", false);
        check("=0.0.3", "0.0.4", false);
    }

    #[test]
    fn wildcards() {
        check("*", "0.0.1", true);
        check("*", "99.1.2", true);
        check("1.*", "1.7.0", true);
        check("1.*", "2.0.0", false);
        check("1.2.*", "1.2.9", true);
        check("1.2.*", "1.3.0", false);
    }

    #[test]
    fn broken_requirements_match_nothing() {
        for bad in ["", "^", "1..2", "^1.2.3.4", "=1.2", "~x", "^1.2.3-beta"] {
            assert!(Req::parse(bad).is_none(), "требование `{bad}` не должно разбираться");
        }
        check("^1.2.3", "1.2", false);
    }
}
