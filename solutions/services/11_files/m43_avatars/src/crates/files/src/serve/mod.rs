use crate::image::sniff;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disposition {
    Inline,
    Attachment,
}

fn ascii_name(name: &str) -> String {
    let last = name.rsplit(['/', '\\']).next().unwrap_or_default();
    let cleaned: String = last
        .chars()
        .filter(|c| c.is_ascii_graphic() && !matches!(c, '"' | '\\' | ';'))
        .take(100)
        .collect();
    if cleaned.is_empty() {
        String::from("file")
    } else {
        cleaned
    }
}

fn percent(name: &str) -> String {
    name.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => {
                (byte as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

pub fn safe_headers(head: &[u8], name: &str) -> (Disposition, Vec<(&'static str, String)>) {
    let (content_type, disposition) = match sniff(head) {
        Some(kind) => (kind.mime(), Disposition::Inline),
        None => ("application/octet-stream", Disposition::Attachment),
    };
    let how = match disposition {
        Disposition::Inline => "inline",
        Disposition::Attachment => "attachment",
    };
    let ascii = ascii_name(name);
    let headers = vec![
        ("Content-Type", content_type.to_string()),
        (
            "Content-Disposition",
            format!(
                "{how}; filename=\"{ascii}\"; filename*=UTF-8''{}",
                percent(&ascii)
            ),
        ),
        ("X-Content-Type-Options", String::from("nosniff")),
        (
            "Content-Security-Policy",
            String::from("default-src 'none'; sandbox"),
        ),
    ];
    (disposition, headers)
}
