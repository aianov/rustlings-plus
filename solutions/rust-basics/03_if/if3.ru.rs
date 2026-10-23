fn animal_habitat(animal: &str) -> &str {
    let identifier = if animal == "crab" {
        1
    } else if animal == "gopher" {
        // Целое число, чтобы у всех веток был один тип.
        2
    } else if animal == "snake" {
        3
    } else {
        // Любой неиспользуемый идентификатор.
        4
    };

    // Вместо такого идентификатора в Rust используют enum.
    // Но до enum мы ещё не дошли.
    if identifier == 1 {
        "Beach"
    } else if identifier == 2 {
        "Burrow"
    } else if identifier == 3 {
        "Desert"
    } else {
        "Unknown"
    }
}

fn main() {
    // Здесь можно по желанию поэкспериментировать.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gopher_lives_in_burrow() {
        assert_eq!(animal_habitat("gopher"), "Burrow")
    }

    #[test]
    fn snake_lives_in_desert() {
        assert_eq!(animal_habitat("snake"), "Desert")
    }

    #[test]
    fn crab_lives_on_beach() {
        assert_eq!(animal_habitat("crab"), "Beach")
    }

    #[test]
    fn unknown_animal() {
        assert_eq!(animal_habitat("dinosaur"), "Unknown")
    }
}
