#[derive(Clone, Copy, Debug)]
enum Item {
    Range(char, char),
    Digit,
    Word,
    Space,
}

impl Item {
    fn has(self, c: char) -> bool {
        match self {
            Item::Range(a, b) => a <= c && c <= b,
            Item::Digit => c.is_ascii_digit(),
            Item::Word => c.is_alphanumeric() || c == '_',
            Item::Space => c.is_whitespace(),
        }
    }
}

#[derive(Debug)]
enum Atom {
    Lit(char),
    Any,
    Class(Vec<Item>, bool),
    Start,
    End,
    Boundary,
}

impl Atom {
    fn one(&self, c: char) -> bool {
        match self {
            Atom::Lit(l) => *l == c,
            Atom::Any => c != '\n',
            Atom::Class(items, neg) => items.iter().any(|i| i.has(c)) != *neg,
            Atom::Start | Atom::End | Atom::Boundary => false,
        }
    }
}

#[derive(Debug)]
struct Node {
    atom: Atom,
    min: usize,
    max: usize,
}

#[derive(Debug)]
pub struct Regex(Vec<Vec<Node>>);

fn shorthand(c: char) -> Option<(Item, bool)> {
    match c {
        'd' => Some((Item::Digit, false)),
        'D' => Some((Item::Digit, true)),
        'w' => Some((Item::Word, false)),
        'W' => Some((Item::Word, true)),
        's' => Some((Item::Space, false)),
        'S' => Some((Item::Space, true)),
        _ => None,
    }
}

fn escaped(c: char) -> char {
    match c {
        'n' => '\n',
        't' => '\t',
        'r' => '\r',
        other => other,
    }
}

fn branches(p: &[char]) -> Vec<&[char]> {
    let mut found = Vec::new();
    let (mut from, mut i, mut class) = (0, 0, false);
    while i < p.len() {
        match p[i] {
            '\\' => i += 1,
            '[' if !class => {
                class = true;
                if p.get(i + 1) == Some(&'^') {
                    i += 1;
                }
                if p.get(i + 1) == Some(&']') {
                    i += 1;
                }
            }
            ']' if class => class = false,
            '|' if !class => {
                found.push(&p[from..i]);
                from = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    found.push(&p[from..]);
    found
}

impl Regex {
    pub fn new(pattern: &str) -> Result<Regex, String> {
        let p: Vec<char> = pattern.chars().collect();
        let mut alternatives = Vec::new();
        for branch in branches(&p) {
            if branch.is_empty() {
                return Err(format!("regex `{pattern}`: empty alternative"));
            }
            alternatives.push(Regex::branch(pattern, branch)?);
        }
        Ok(Regex(alternatives))
    }

    fn branch(pattern: &str, p: &[char]) -> Result<Vec<Node>, String> {
        let mut nodes: Vec<Node> = Vec::new();
        let mut i = 0;
        while i < p.len() {
            let c = p[i];
            i += 1;
            let atom = match c {
                '.' => Atom::Any,
                '^' => Atom::Start,
                '$' => Atom::End,
                '*' | '+' | '?' => {
                    return Err(format!("regex `{pattern}`: nothing to repeat before `{c}`"));
                }
                '\\' => {
                    let Some(&e) = p.get(i) else {
                        return Err(format!("regex `{pattern}`: trailing backslash"));
                    };
                    i += 1;
                    match shorthand(e) {
                        Some((item, neg)) => Atom::Class(vec![item], neg),
                        None if e == 'b' => Atom::Boundary,
                        None => Atom::Lit(escaped(e)),
                    }
                }
                '[' => {
                    let mut neg = false;
                    if p.get(i) == Some(&'^') {
                        neg = true;
                        i += 1;
                    }
                    let mut items = Vec::new();
                    let mut closed = false;
                    let mut first = true;
                    while i < p.len() {
                        let mut ch = p[i];
                        i += 1;
                        if ch == ']' && !first {
                            closed = true;
                            break;
                        }
                        first = false;
                        if ch == '\\' {
                            let Some(&e) = p.get(i) else {
                                return Err(format!("regex `{pattern}`: trailing backslash"));
                            };
                            i += 1;
                            if let Some((item, n)) = shorthand(e) {
                                if n {
                                    return Err(format!(
                                        "regex `{pattern}`: negated shorthand inside class"
                                    ));
                                }
                                items.push(item);
                                continue;
                            }
                            ch = escaped(e);
                        }
                        if p.get(i) == Some(&'-') && p.get(i + 1).is_some_and(|&n| n != ']') {
                            let mut hi = p[i + 1];
                            i += 2;
                            if hi == '\\' {
                                let Some(&e) = p.get(i) else {
                                    return Err(format!("regex `{pattern}`: trailing backslash"));
                                };
                                i += 1;
                                hi = escaped(e);
                            }
                            if hi < ch {
                                return Err(format!("regex `{pattern}`: bad range"));
                            }
                            items.push(Item::Range(ch, hi));
                        } else {
                            items.push(Item::Range(ch, ch));
                        }
                    }
                    if !closed {
                        return Err(format!("regex `{pattern}`: unclosed `[`"));
                    }
                    Atom::Class(items, neg)
                }
                other => Atom::Lit(other),
            };
            let (min, max) = match p.get(i) {
                Some('*') => (0, usize::MAX),
                Some('+') => (1, usize::MAX),
                Some('?') => (0, 1),
                _ => (1, 1),
            };
            if (min, max) != (1, 1) {
                i += 1;
                if matches!(atom, Atom::Start | Atom::End | Atom::Boundary) {
                    return Err(format!("regex `{pattern}`: cannot repeat an anchor"));
                }
            }
            nodes.push(Node { atom, min, max });
        }
        Ok(nodes)
    }

    pub fn find_line(&self, text: &str) -> Option<usize> {
        let t: Vec<char> = text.chars().collect();
        let mut line = 1;
        for start in 0..=t.len() {
            if self.0.iter().any(|nodes| rec(nodes, &t, start)) {
                return Some(line);
            }
            if start < t.len() && t[start] == '\n' {
                line += 1;
            }
        }
        None
    }

    #[cfg(test)]
    pub fn is_match(&self, text: &str) -> bool {
        self.find_line(text).is_some()
    }
}

fn rec(nodes: &[Node], t: &[char], i: usize) -> bool {
    let Some((n, rest)) = nodes.split_first() else {
        return true;
    };
    match n.atom {
        Atom::Start => (i == 0 || t[i - 1] == '\n') && rec(rest, t, i),
        Atom::End => (i == t.len() || t[i] == '\n') && rec(rest, t, i),
        Atom::Boundary => {
            let word = |c: char| c.is_alphanumeric() || c == '_';
            let before = i > 0 && word(t[i - 1]);
            let after = i < t.len() && word(t[i]);
            before != after && rec(rest, t, i)
        }
        _ => {
            let mut count = 0;
            while count < n.max && i + count < t.len() && n.atom.one(t[i + count]) {
                count += 1;
            }
            if count < n.min {
                return false;
            }
            (n.min..=count).rev().any(|j| rec(rest, t, i + j))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Regex;

    fn m(p: &str, t: &str) -> bool {
        Regex::new(p).unwrap().is_match(t)
    }

    #[test]
    fn literals_and_quantifiers() {
        assert!(m("TcpListener::bind", "let l = TcpListener::bind(a);"));
        assert!(m("\\.unwrap\\(\\)", "x.unwrap()"));
        assert!(!m("\\.unwrap\\(\\)", "x.unwrap_or(1)"));
        assert!(m("ab*c", "ac"));
        assert!(m("ab+c", "abbc"));
        assert!(!m("ab+c", "ac"));
        assert!(m("colou?r", "color"));
    }

    #[test]
    fn classes_and_shorthands() {
        assert!(m("[a-c]+x", "zzbcx"));
        assert!(!m("[^a-c]x", "ax"));
        assert!(m("\\d+", "v12"));
        assert!(m("\\w+\\s\\w+", "ab cd"));
        assert!(m("[\\d.]+", "3.14"));
        assert!(m("[]]", "]"));
    }

    #[test]
    fn anchors_and_dot() {
        assert!(m("^fn main", "use a;\nfn main() {}"));
        assert!(!m("^main", "fn main"));
        assert!(m("end$", "the end\nnext"));
        assert!(m("a.c", "abc"));
        assert!(!m("a.c", "a\nc"));
    }

    #[test]
    fn alternatives_and_boundaries() {
        assert!(m(
            "seed_from_u64|thread_rng|SmallRng",
            "let r = thread_rng();"
        ));
        assert!(!m(
            "seed_from_u64|thread_rng|SmallRng",
            "let r = getrandom::fill(&mut b);"
        ));
        assert!(m("[|]x", "a|x"));
        assert!(m("a\\|b", "a|b"));
        assert!(!m("a\\|b", "b"));
        assert!(m("\\bApp::", "let a = App::new();"));
        assert!(!m("\\bApp::", "let a = MyApp::new();"));
        assert!(m("^App::", "App::new()"));
        assert_eq!(Regex::new("x|y").unwrap().find_line("a\ny\nx"), Some(2));
    }

    #[test]
    fn line_number_and_errors() {
        assert_eq!(Regex::new("x").unwrap().find_line("a\nb\nx"), Some(3));
        assert!(Regex::new("*a").is_err());
        assert!(Regex::new("[a").is_err());
        assert!(Regex::new("a\\").is_err());
        assert!(Regex::new("a|").is_err());
        assert!(Regex::new("\\b+").is_err());
    }
}
