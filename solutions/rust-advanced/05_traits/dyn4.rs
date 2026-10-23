pub trait Shape {
    fn area(&self) -> f64;
    fn name(&self) -> &str;
}

pub struct Circle(pub f64);
pub struct Rect(pub f64, pub f64);

impl Shape for Circle {
    fn area(&self) -> f64 {
        std::f64::consts::PI * self.0 * self.0
    }

    fn name(&self) -> &str {
        "circle"
    }
}

impl Shape for Rect {
    fn area(&self) -> f64 {
        self.0 * self.1
    }

    fn name(&self) -> &str {
        "rect"
    }
}

pub fn total_generic<S: Shape>(items: &[S]) -> f64 {
    items.iter().map(Shape::area).sum()
}

pub fn total_dyn(items: &[Box<dyn Shape>]) -> f64 {
    items.iter().map(|s| s.area()).sum()
}

pub fn largest(items: &[Box<dyn Shape>]) -> Option<&dyn Shape> {
    items
        .iter()
        .map(|s| s.as_ref())
        .max_by(|a, b| a.area().total_cmp(&b.area()))
}

pub fn names_over(items: &[Box<dyn Shape>], min_area: f64) -> Vec<&str> {
    items.iter().filter(|s| s.area() > min_area).map(|s| s.name()).collect()
}

fn main() {
    println!("{}", total_generic(&[Rect(1.0, 2.0)]));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mixed() -> Vec<Box<dyn Shape>> {
        vec![Box::new(Rect(2.0, 3.0)), Box::new(Circle(1.0)), Box::new(Rect(10.0, 1.0))]
    }

    #[test]
    fn generic_total_for_one_type() {
        assert_eq!(total_generic(&[Rect(1.0, 2.0), Rect(3.0, 4.0)]), 14.0);
        assert_eq!(total_generic::<Circle>(&[]), 0.0);
    }

    #[test]
    fn dyn_total_for_mixed() {
        let expected = 6.0 + std::f64::consts::PI + 10.0;
        assert!((total_dyn(&mixed()) - expected).abs() < 1e-9, "сумма площадей смешанного списка");
        assert_eq!(total_dyn(&[]), 0.0);
    }

    #[test]
    fn largest_is_a_borrow_from_the_list() {
        let list = mixed();
        let big = largest(&list).expect("список не пуст");
        assert_eq!(big.area(), 10.0);
        assert_eq!(big.name(), "rect");
        assert!(largest(&[]).is_none(), "в пустом списке наибольшей нет");
    }

    #[test]
    fn names_over_threshold() {
        assert_eq!(names_over(&mixed(), 5.0), vec!["rect", "rect"]);
        assert_eq!(names_over(&mixed(), 6.0), vec!["rect"], "порог строгий: площадь ровно 6 не проходит");
        assert!(names_over(&mixed(), 100.0).is_empty());
    }
}
