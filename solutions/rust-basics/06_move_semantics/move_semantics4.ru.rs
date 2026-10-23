fn main() {
    // Здесь можно по желанию поэкспериментировать.
}

#[cfg(test)]
mod tests {
    #[test]
    fn move_semantics4() {
        let mut x = Vec::new();
        let y = &mut x;
        // Здесь используется `y`.
        y.push(42);
        // Изменяемая ссылка `y` больше не используется,
        // поэтому можно создать новую ссылку.
        let z = &mut x;
        z.push(13);
        assert_eq!(x, [42, 13]);
    }
}
