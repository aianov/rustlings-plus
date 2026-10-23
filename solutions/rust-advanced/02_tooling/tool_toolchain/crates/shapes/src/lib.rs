#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub fn new(width: f64, height: f64) -> Rect {
        Rect { width, height }
    }

    pub fn area(&self) -> f64 {
        self.width * self.height
    }

    pub fn describe(&self) -> String {
        format!(
            "rect {} by {} with area {} and perimeter {}",
            self.width,
            self.height,
            self.area(),
            self.perimeter()
        )
    }

    pub fn perimeter(&self) -> f64 {
        2.0 * (self.width + self.height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_itself() {
        assert_eq!(
            Rect::new(2.0, 3.0).describe(),
            "rect 2 by 3 with area 6 and perimeter 10"
        );
    }
}
