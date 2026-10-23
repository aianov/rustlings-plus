fn answer() -> i32 {
    42
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fail() {
        assert_eq!(answer(), 42);
    }
}
