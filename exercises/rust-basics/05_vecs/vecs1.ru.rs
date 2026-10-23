fn elems_to_vec(a: i32, b: i32, c: i32) -> Vec<i32> {
    // TODO: Верни вектор из элементов a, b и c (в таком порядке).
    // Используй макрос "vec!".
}

fn main() {
    // Здесь можно по желанию поэкспериментировать.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_elems_to_vec() {
        let (a, b, c) = (2, 7, 12);
        let v = elems_to_vec(a, b, c);
        assert_eq!(v[0], a);
        assert_eq!(v[1], b);
        assert_eq!(v[2], c);
    }
}
