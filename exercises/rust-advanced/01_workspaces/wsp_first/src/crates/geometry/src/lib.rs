#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn new(w: f64, h: f64) -> Rect {
        Rect { w, h }
    }

    pub fn area(&self) -> f64 {
        self.w * self.h
    }

    pub fn perimeter(&self) -> f64 {
        2.0 * (self.w + self.h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_math() {
        let r = Rect::new(3.0, 4.0);
        assert_eq!(r.area(), 12.0);
        assert_eq!(r.perimeter(), 14.0);
    }
}
