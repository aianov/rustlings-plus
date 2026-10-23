fn main() {
    // Здесь можно по желанию поэкспериментировать.
}

#[cfg(test)]
mod tests {
    #[test]
    fn indexing_tuple() {
        let numbers = (1, 2, 3);

        // Синтаксис индексации кортежа.
        let second = numbers.1;

        assert_eq!(second, 2, "Это не второе число в кортеже!");
    }
}
