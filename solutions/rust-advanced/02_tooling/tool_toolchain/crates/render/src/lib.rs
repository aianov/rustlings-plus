use shapes::Rect;

pub fn banner(rect: &Rect) -> String {
    format!("=== {} ===", rect.describe())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_the_description() {
        assert_eq!(
            banner(&Rect::new(1.0, 1.0)),
            "=== rect 1 by 1 with area 1 and perimeter 4 ==="
        );
    }
}
