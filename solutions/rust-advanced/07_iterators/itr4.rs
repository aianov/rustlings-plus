pub struct Secret(pub String);

pub struct Token {
    pub id: u64,
    pub secret: Secret,
}

pub fn bump_ids(counters: &mut [u64]) {
    for c in counters.iter_mut() {
        *c += 1;
    }
}

pub fn longest_secret(tokens: &[Token]) -> Option<&Token> {
    tokens.iter().rev().max_by_key(|t| t.secret.0.len())
}

pub fn rename_all(tokens: &mut [Token], suffix: &str) {
    for t in tokens.iter_mut() {
        t.secret.0.push_str(suffix);
    }
}

pub fn into_secrets(tokens: Vec<Token>) -> Vec<Secret> {
    tokens.into_iter().map(|t| t.secret).collect()
}

pub fn ids(tokens: &[Token]) -> Vec<u64> {
    tokens.iter().map(|t| t.id).collect()
}

fn main() {
    let mut c = [1, 2];
    bump_ids(&mut c);
    println!("{c:?}");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens() -> Vec<Token> {
        vec![
            Token { id: 1, secret: Secret("abc".into()) },
            Token { id: 2, secret: Secret("abcd".into()) },
            Token { id: 3, secret: Secret("wxyz".into()) },
            Token { id: 4, secret: Secret("a".into()) },
        ]
    }

    #[test]
    fn counters_change_in_place() {
        let mut c = [0, 5, 9];
        bump_ids(&mut c);
        assert_eq!(c, [1, 6, 10]);
    }

    #[test]
    fn longest_is_the_first_of_equals() {
        let t = tokens();
        assert_eq!(longest_secret(&t).map(|t| t.id), Some(2), "у токенов 2 и 3 секреты одной длины: нужен первый");
        assert!(longest_secret(&[]).is_none());
    }

    #[test]
    fn rename_edits_the_originals() {
        let mut t = tokens();
        rename_all(&mut t, "!");
        assert_eq!(t[0].secret.0, "abc!");
        assert_eq!(t[3].secret.0, "a!");
    }

    #[test]
    fn secrets_are_moved_out() {
        let secrets: Vec<String> = into_secrets(tokens()).into_iter().map(|s| s.0).collect();
        assert_eq!(secrets, vec!["abc", "abcd", "wxyz", "a"]);
    }

    #[test]
    fn ids_leave_the_tokens_alone() {
        let t = tokens();
        assert_eq!(ids(&t), vec![1, 2, 3, 4]);
        assert_eq!(t.len(), 4, "токены остались у владельца");
    }
}
