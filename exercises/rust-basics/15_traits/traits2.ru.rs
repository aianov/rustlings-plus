trait AppendBar {
    fn append_bar(self) -> Self;
}

// TODO: Реализуй трейт `AppendBar` для вектора строк.
// `append_bar` должна добавлять в вектор строку "Bar".

fn main() {
    // Здесь можно поэкспериментировать.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_vec_pop_eq_bar() {
        let mut foo = vec![String::from("Foo")].append_bar();
        assert_eq!(foo.pop().unwrap(), "Bar");
        assert_eq!(foo.pop().unwrap(), "Foo");
    }
}
