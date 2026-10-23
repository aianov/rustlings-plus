fn fill_vec(vec: Vec<i32>) -> Vec<i32> {
    let mut vec = vec;

    vec.push(88);

    vec
}

fn main() {
    // Здесь можно по желанию поэкспериментировать.
}

#[cfg(test)]
mod tests {
    use super::*;

    // TODO: Сделай оба вектора, `vec0` и `vec1`, доступными одновременно, чтобы
    // исправить ошибку компилятора в тесте.
    #[test]
    fn move_semantics2() {
        let vec0 = vec![22, 44, 66];

        let vec1 = fill_vec(vec0);

        assert_eq!(vec0, [22, 44, 66]);
        assert_eq!(vec1, [22, 44, 66, 88]);
    }
}
