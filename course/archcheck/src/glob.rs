pub fn matches(pattern: &str, path: &str) -> bool {
    let pattern = pattern.trim_start_matches("./").trim_end_matches('/');
    let p: Vec<&str> = pattern.split('/').collect();
    let s: Vec<&str> = path.split('/').collect();
    segments(&p, &s)
}

fn segments(p: &[&str], s: &[&str]) -> bool {
    match p.split_first() {
        None => s.is_empty(),
        Some((&"**", rest)) => (0..=s.len()).any(|i| segments(rest, &s[i..])),
        Some((first, rest)) => match s.split_first() {
            Some((head, tail)) => segment(first, head) && segments(rest, tail),
            None => false,
        },
    }
}

fn segment(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let n: Vec<char> = name.chars().collect();
    let (mut pi, mut ni) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while ni < n.len() {
        if pi < p.len() && p[pi] == '*' {
            star = Some((pi, ni));
            pi += 1;
        } else if pi < p.len() && (p[pi] == '?' || p[pi] == n[ni]) {
            pi += 1;
            ni += 1;
        } else if let Some((sp, sn)) = star {
            pi = sp + 1;
            ni = sn + 1;
            star = Some((sp, sn + 1));
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

#[cfg(test)]
mod tests {
    use super::matches;

    #[test]
    fn star_and_question() {
        assert!(matches("src/*.rs", "src/a.rs"));
        assert!(!matches("src/*.rs", "src/a/b.rs"));
        assert!(matches("a?c", "abc"));
        assert!(!matches("a?c", "ac"));
        assert!(matches("*.rs", "x.rs"));
        assert!(matches("a*b*c", "aXXbYYc"));
        assert!(!matches("a*b*c", "aXXbYY"));
    }

    #[test]
    fn double_star() {
        assert!(matches("**/dto/**", "src/dto/a.rs"));
        assert!(matches("**/dto/**", "dto"));
        assert!(matches("**/utils.rs", "utils.rs"));
        assert!(matches("**/utils.rs", "a/b/utils.rs"));
        assert!(matches("src/**/*.rs", "src/a.rs"));
        assert!(matches("src/**/*.rs", "src/a/b/c.rs"));
        assert!(!matches("src/**/*.rs", "lib/a.rs"));
        assert!(matches("./src/crates/*", "src/crates/x"));
    }
}
