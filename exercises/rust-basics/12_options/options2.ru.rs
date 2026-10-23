fn main() {
    // Здесь можно поэкспериментировать.
}

#[cfg(test)]
mod tests {
    #[test]
    fn simple_option() {
        let target = "rustlings";
        let optional_target = Some(target);

        // TODO: Преврати это в оператор if-let со значением `Some`.
        word = optional_target {
            assert_eq!(word, target);
        }
    }

    #[test]
    fn layered_option() {
        let range = 10;
        let mut optional_integers: Vec<Option<i8>> = vec![None];

        for i in 1..=range {
            optional_integers.push(Some(i));
        }

        let mut cursor = range;

        // TODO: Преврати это в while-let. Помни, что `Vec::pop()` добавляет
        // ещё один слой `Option`. В if-let и while-let можно вкладывать шаблоны.
        // Подсказка: слои вкладываются прямо в шаблон.
        integer = optional_integers.pop() {
            assert_eq!(integer, cursor);
            cursor -= 1;
        }

        assert_eq!(cursor, 0);
    }
}
