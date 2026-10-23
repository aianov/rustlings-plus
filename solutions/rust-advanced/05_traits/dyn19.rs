use std::any::{Any, TypeId};

#[derive(Default)]
pub struct Bag {
    items: Vec<Box<dyn Any>>,
}

impl Bag {
    pub fn new() -> Bag {
        Bag::default()
    }

    pub fn add<T: Any>(&mut self, value: T) {
        self.items.push(Box::new(value));
    }

    pub fn count_of<T: Any>(&self) -> usize {
        self.items.iter().filter(|item| is_type::<T>(item.as_ref())).count()
    }

    pub fn first_of<T: Any>(&self) -> Option<&T> {
        self.items.iter().find_map(|item| item.downcast_ref::<T>())
    }
}

fn is_type<T: Any>(value: &dyn Any) -> bool {
    value.type_id() == TypeId::of::<T>()
}

fn main() {
    let mut bag = Bag::new();
    bag.add(1u8);
    println!("{}", bag.count_of::<u8>());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_by_type() {
        let mut bag = Bag::new();
        bag.add(1i32);
        bag.add("hi".to_string());
        bag.add(2i32);
        bag.add(1.5f64);
        bag.add(3i32);
        assert_eq!(bag.count_of::<i32>(), 3, "три значения типа i32");
        assert_eq!(bag.count_of::<String>(), 1);
        assert_eq!(bag.count_of::<f64>(), 1);
        assert_eq!(bag.count_of::<u8>(), 0);
    }

    #[test]
    fn first_of_finds_the_right_one() {
        let mut bag = Bag::new();
        bag.add("a".to_string());
        bag.add(5i32);
        bag.add(6i32);
        assert_eq!(bag.first_of::<i32>(), Some(&5));
        assert_eq!(bag.first_of::<u64>(), None);
    }

    #[test]
    fn nested_boxes_are_just_values() {
        let mut bag = Bag::new();
        bag.add(Box::new(1i32));
        assert_eq!(bag.count_of::<Box<i32>>(), 1, "Box<i32> тоже просто значение одного типа");
        assert_eq!(bag.count_of::<i32>(), 0, "а i32 внутри ящика это уже другой тип");
    }
}
