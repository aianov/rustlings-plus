fn main() {
    // Здесь можно по желанию поэкспериментировать.
}

#[cfg(test)]
mod tests {
    #[test]
    fn slice_out_of_array() {
        let a = [1, 2, 3, 4, 5];

        // TODO: Получи из массива `a` срез `nice_slice`, чтобы тест прошёл.
        // let nice_slice = ???

        assert_eq!([2, 3, 4], nice_slice);
    }
}
