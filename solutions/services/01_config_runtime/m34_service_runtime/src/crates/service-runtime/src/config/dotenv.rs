use super::Problem;

fn is_key(key: &str) -> bool {
    let mut chars = key.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn unquote(value: &str) -> &str {
    for quote in ['"', '\''] {
        if let Some(inner) = value
            .strip_prefix(quote)
            .and_then(|rest| rest.strip_suffix(quote))
        {
            return inner;
        }
    }
    match value.find(" #") {
        Some(at) => value[..at].trim_end(),
        None => value,
    }
}

pub fn parse_dotenv(text: &str) -> Result<Vec<(String, String)>, Vec<Problem>> {
    let mut pairs = Vec::new();
    let mut problems = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        match line.split_once('=') {
            Some((key, value)) if is_key(key.trim()) => {
                pairs.push((key.trim().to_string(), unquote(value.trim()).to_string()));
            }
            _ => problems.push(Problem::Dotenv { line: index + 1 }),
        }
    }
    if problems.is_empty() {
        Ok(pairs)
    } else {
        Err(problems)
    }
}
