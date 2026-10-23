pub fn apply(op: &str, a: i64, b: i64) -> Option<i64> {
    match op {
        "+" => a.checked_add(b),
        "-" => a.checked_sub(b),
        "*" => a.checked_mul(b),
        "/" => a.checked_div(b),
        _ => None,
    }
}

pub fn eval(expr: &str) -> Option<i64> {
    let mut parts = expr.split_whitespace();
    let a = parts.next()?.parse::<i64>().ok()?;
    let op = parts.next()?;
    let b = parts.next()?.parse::<i64>().ok()?;
    if let Some(result) = apply(op, a, b)
        && parts.next().is_none()
    {
        Some(result)
    } else {
        None
    }
}
