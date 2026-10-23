fn main() {
    // Здесь можно по желанию поэкспериментировать.
}

#[cfg(test)]
mod tests {
    #[test]
    fn indexing_tuple() {
        let numbers = (1, 2, 3);

        // TODO: Используй индекс кортежа, чтобы получить второй элемент `numbers`
        // и присвой его переменной `second`.
        // let second = ???;

        assert_eq!(second, 2, "Это не второе число в кортеже!");
    }
}
