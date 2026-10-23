use std::time::Duration;

use tonic::Request;

pub fn parse_grpc_timeout(value: &str) -> Option<Duration> {
    let unit = value.chars().last()?;
    let digits = &value[..value.len() - unit.len_utf8()];
    if digits.is_empty() || digits.len() > 8 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n: u64 = digits.parse().ok()?;
    Some(match unit {
        'H' => Duration::from_secs(n * 3600),
        'M' => Duration::from_secs(n * 60),
        'S' => Duration::from_secs(n),
        'm' => Duration::from_millis(n),
        'u' => Duration::from_micros(n),
        'n' => Duration::from_nanos(n),
        _ => return None,
    })
}

pub fn budget_of<T>(request: &Request<T>) -> Option<Duration> {
    let value = request.metadata().get("grpc-timeout")?.to_str().ok()?;
    parse_grpc_timeout(value)
}
