pub trait Shape: ShapeClone {
    fn area(&self) -> f64;
    fn scale(&mut self, k: f64);
}

pub trait ShapeClone {
    fn clone_box(&self) -> Box<dyn Shape>;
}

impl<T: Shape + Clone + 'static> ShapeClone for T {
    fn clone_box(&self) -> Box<dyn Shape> {
        Box::new(self.clone())
    }
}

impl Clone for Box<dyn Shape> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

#[derive(Clone)]
pub struct Square(pub f64);

#[derive(Clone)]
pub struct Circle(pub f64);

impl Shape for Square {
    fn area(&self) -> f64 {
        self.0 * self.0
    }

    fn scale(&mut self, k: f64) {
        self.0 *= k;
    }
}

impl Shape for Circle {
    fn area(&self) -> f64 {
        std::f64::consts::PI * self.0 * self.0
    }

    fn scale(&mut self, k: f64) {
        self.0 *= k;
    }
}

pub fn total(canvas: &[Box<dyn Shape>]) -> f64 {
    canvas.iter().map(|s| s.area()).sum()
}

fn main() {
    let canvas: Vec<Box<dyn Shape>> = vec![Box::new(Square(2.0))];
    println!("{}", total(&canvas.clone()));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canvas() -> Vec<Box<dyn Shape>> {
        vec![Box::new(Square(2.0)), Box::new(Circle(1.0))]
    }

    #[test]
    fn vector_of_boxes_clones() {
        let original = canvas();
        let copy = original.clone();
        assert_eq!(copy.len(), 2);
        assert!((total(&copy) - total(&original)).abs() < 1e-9, "копия должна иметь ту же площадь");
    }

    #[test]
    fn copies_are_independent() {
        let original = canvas();
        let mut copy = original.clone();
        for shape in copy.iter_mut() {
            shape.scale(10.0);
        }
        assert!((total(&original) - (4.0 + std::f64::consts::PI)).abs() < 1e-9, "изменение копии не должно задевать оригинал");
        assert!(total(&copy) > 100.0 * total(&original) * 0.99, "копия выросла в сто раз по площади");
    }

    #[test]
    fn single_box_clones_with_its_real_type() {
        let b: Box<dyn Shape> = Box::new(Square(3.0));
        let c = b.clone();
        assert_eq!(c.area(), 9.0, "клон остаётся квадратом, а не обрезается до общего трейта");
    }
}
