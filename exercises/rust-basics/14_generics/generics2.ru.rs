// Эта мощная обёртка умеет хранить положительное целое число.
// TODO: Перепиши её с обобщением, чтобы она могла хранить ЛЮБОЙ тип.
struct Wrapper {
    value: u32,
}

// TODO: Сделай реализацию структуры обобщённой по хранимому значению.
impl Wrapper {
    fn new(value: u32) -> Self {
        Wrapper { value }
    }
}

fn main() {
    // Здесь можно поэкспериментировать.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_u32_in_wrapper() {
        assert_eq!(Wrapper::new(42).value, 42);
    }

    #[test]
    fn store_str_in_wrapper() {
        assert_eq!(Wrapper::new("Foo").value, "Foo");
    }
}
