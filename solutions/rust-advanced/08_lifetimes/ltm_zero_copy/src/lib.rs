use std::borrow::Cow;
use std::ops::Range;

#[derive(Debug, PartialEq)]
pub struct LogError {
    pub line: usize,
    pub reason: &'static str,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entry<'a> {
    pub ip: &'a str,
    pub method: &'a str,
    pub target: &'a str,
    pub status: u16,
    pub agent: Cow<'a, str>,
}

pub struct AccessLog {
    buffer: String,
    lines: Vec<Range<usize>>,
}

impl<'a> Entry<'a> {
    pub fn path(&self) -> &'a str {
        self.target.split_once('?').map_or(self.target, |(path, _)| path)
    }

    pub fn query(&self) -> Option<&'a str> {
        self.target.split_once('?').map(|(_, query)| query)
    }
}

fn next_field<'a>(rest: &mut &'a str) -> Option<&'a str> {
    let trimmed = rest.trim_start_matches(' ');
    if trimmed.is_empty() {
        return None;
    }
    let end = trimmed.find(' ').unwrap_or(trimmed.len());
    let (field, tail) = trimmed.split_at(end);
    *rest = tail;
    Some(field)
}

fn parse_agent(text: &str) -> Result<Cow<'_, str>, &'static str> {
    let body = text.strip_prefix('"').ok_or("bad agent")?;
    let mut escaped = false;
    let mut end = None;
    for (i, c) in body.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '\\' => escaped = true,
            '"' => {
                end = Some(i);
                break;
            }
            _ => {}
        }
    }
    let end = end.ok_or("bad agent")?;
    if !body[end + 1..].trim().is_empty() {
        return Err("bad agent");
    }
    let inner = &body[..end];
    if !inner.contains('\\') {
        return Ok(Cow::Borrowed(inner));
    }
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next_if(|n| *n == '"' || *n == '\\') {
                Some(next) => out.push(next),
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    Ok(Cow::Owned(out))
}

fn parse_line(line: &str) -> Result<Entry<'_>, &'static str> {
    let mut rest = line;
    let ip = next_field(&mut rest).ok_or("missing field")?;
    let method = next_field(&mut rest).ok_or("missing field")?;
    let target = next_field(&mut rest).ok_or("missing field")?;
    let status = next_field(&mut rest).ok_or("missing field")?.parse::<u16>().map_err(|_| "bad status")?;
    let agent = parse_agent(rest.trim_start_matches(' '))?;
    Ok(Entry { ip, method, target, status, agent })
}

impl AccessLog {
    pub fn new(buffer: String) -> Result<AccessLog, LogError> {
        let mut lines = Vec::with_capacity(buffer.matches('\n').count() + 1);
        let mut offset = 0;
        for (index, raw) in buffer.split_inclusive('\n').enumerate() {
            let start = offset;
            offset += raw.len();
            let text = raw.trim_end_matches(['\n', '\r']);
            if text.trim().is_empty() {
                continue;
            }
            parse_line(text).map_err(|reason| LogError { line: index + 1, reason })?;
            lines.push(start..start + text.len());
        }
        Ok(AccessLog { buffer, lines })
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    pub fn raw(&self) -> &str {
        &self.buffer
    }

    pub fn get(&self, index: usize) -> Option<Entry<'_>> {
        let range = self.lines.get(index)?;
        parse_line(&self.buffer[range.clone()]).ok()
    }

    pub fn entries(&self) -> impl Iterator<Item = Entry<'_>> {
        self.lines.iter().filter_map(|range| parse_line(&self.buffer[range.clone()]).ok())
    }

    pub fn with_status(&self, min: u16, max: u16) -> impl Iterator<Item = Entry<'_>> {
        self.entries().filter(move |e| (min..=max).contains(&e.status))
    }
}
