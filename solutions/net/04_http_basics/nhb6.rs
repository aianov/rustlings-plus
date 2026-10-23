#[derive(Debug, PartialEq)]
enum ParseError {
    BadHeader,
}

fn parse_header_line(line: &str) -> Result<(String, String), ParseError> {
    let (name, value) = line.split_once(':').ok_or(ParseError::BadHeader)?;
    let is_token = |c: char| c.is_ascii_alphanumeric() || "!#$%&'*+-.^_`|~".contains(c);
    if name.is_empty() || !name.chars().all(is_token) {
        return Err(ParseError::BadHeader);
    }
    let value = value.trim_matches(|c| c == ' ' || c == '\t');
    if value.chars().any(|c| c.is_control() && c != '\t') {
        return Err(ParseError::BadHeader);
    }
    Ok((name.to_string(), value.to_string()))
}

fn main() {
    println!("{:?}", parse_header_line("Content-Type:   text/plain  "));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(name: &str, value: &str) -> Result<(String, String), ParseError> {
        Ok((name.to_string(), value.to_string()))
    }

    #[test]
    fn trims_value_but_keeps_name_case() {
        assert_eq!(parse_header_line("Content-Type: text/plain"), ok("Content-Type", "text/plain"));
        assert_eq!(parse_header_line("X-Id:\t  42 \t "), ok("X-Id", "42"), "пробелы и табуляции по краям значения отбрасываются");
        assert_eq!(parse_header_line("host:example.test"), ok("host", "example.test"), "пробел после двоеточия необязателен");
    }

    #[test]
    fn colons_inside_the_value_are_fine() {
        assert_eq!(parse_header_line("Host: 127.0.0.1:8080"), ok("Host", "127.0.0.1:8080"));
        assert_eq!(parse_header_line("Date: Tue, 07 Oct 2025 10:30:00 GMT"), ok("Date", "Tue, 07 Oct 2025 10:30:00 GMT"));
        assert_eq!(parse_header_line("X-Empty:"), ok("X-Empty", ""), "пустое значение разрешено");
        assert_eq!(parse_header_line("X-Spaces:   "), ok("X-Spaces", ""));
    }

    #[test]
    fn whitespace_in_or_before_the_name_is_rejected() {
        for line in ["Content-Length : 5", " Folded: continuation", "\tFolded: x", "Bad Name: x", "Content-Length\t: 5"] {
            assert_eq!(parse_header_line(line), Err(ParseError::BadHeader), "{line:?}: пробел в имени или перед двоеточием недопустим");
        }
    }

    #[test]
    fn malformed_lines() {
        for line in ["NoColon", "", ": value", "Na(me: x", "Na@me: x", "Name: a\0b", "Name: a\x7fb", "Name: a\rb"] {
            assert_eq!(parse_header_line(line), Err(ParseError::BadHeader), "{line:?} должна быть отвергнута");
        }
    }
}
