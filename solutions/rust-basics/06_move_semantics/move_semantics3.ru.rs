fn fill_vec(mut vec: Vec<i32>) -> Vec<i32> {
    //      ^^^ добавлено
    vec.push(88);

    vec
}

fn main() {
    // Здесь можно по желанию поэкспериментировать.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_semantics3() {
        let vec0 = vec![22, 44, 66];
        let vec1 = fill_vec(vec0);
        assert_eq!(vec1, [22, 44, 66, 88]);
    }
}
