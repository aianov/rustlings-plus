pub fn first_comment_line(src: &str) -> Option<usize> {
    let s: Vec<char> = src.chars().collect();
    let n = s.len();
    let at = |i: usize| s.get(i).copied();
    let is_ident = |c: char| c.is_alphanumeric() || c == '_';
    let line_of = |i: usize| 1 + s[..i].iter().filter(|&&c| c == '\n').count();
    let mut i = 0;
    while i < n {
        let c = s[i];
        if c == '/' && matches!(at(i + 1), Some('/') | Some('*')) {
            return Some(line_of(i));
        }
        if c == '"' {
            i += 1;
            while i < n {
                match s[i] {
                    '\\' => i += 2,
                    '"' => {
                        i += 1;
                        break;
                    }
                    _ => i += 1,
                }
            }
            continue;
        }
        if (c == 'r' || c == 'b') && (i == 0 || !is_ident(s[i - 1])) {
            let mut j = i;
            if s[j] == 'b' {
                j += 1;
            }
            if at(j) == Some('r') {
                j += 1;
                let mut hashes = 0;
                while at(j) == Some('#') {
                    hashes += 1;
                    j += 1;
                }
                if at(j) == Some('"') {
                    j += 1;
                    while j < n {
                        if s[j] == '"' && (1..=hashes).all(|k| at(j + k) == Some('#')) {
                            j += 1 + hashes;
                            break;
                        }
                        j += 1;
                    }
                    i = j;
                    continue;
                }
            }
        }
        if c == '\'' {
            if at(i + 1) == Some('\\') {
                let mut j = i + 3;
                while j < n && s[j] != '\'' {
                    j += 1;
                }
                i = j + 1;
            } else if at(i + 2) == Some('\'') {
                i += 3;
            } else {
                i += 1;
            }
            continue;
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::first_comment_line;

    #[test]
    fn detects_comments() {
        assert_eq!(first_comment_line("fn a() {}\n// hi\n"), Some(2));
        assert_eq!(first_comment_line("/// doc\nfn a() {}"), Some(1));
        assert_eq!(first_comment_line("//! mod\n"), Some(1));
        assert_eq!(first_comment_line("fn a() {}\n/* x */"), Some(2));
    }

    #[test]
    fn ignores_literals() {
        assert_eq!(first_comment_line("let a = \"// not\";"), None);
        assert_eq!(first_comment_line("let a = \"q\\\" // not\";"), None);
        assert_eq!(first_comment_line("let a = r#\"// \" not\"#;"), None);
        assert_eq!(first_comment_line("let a = br\"/* no */\";"), None);
        assert_eq!(first_comment_line("let a = b\"//\";"), None);
        assert_eq!(
            first_comment_line("let a = '/'; let b = '\\''; let c = '\"';"),
            None
        );
        assert_eq!(
            first_comment_line("fn f<'a>(x: &'a str) -> &'a str { x }"),
            None
        );
        assert_eq!(first_comment_line("let a = \"x\"; // yes"), Some(1));
    }
}
