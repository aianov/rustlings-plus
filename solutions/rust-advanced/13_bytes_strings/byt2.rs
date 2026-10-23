pub fn floor_boundary(text: &str, index: usize) -> usize {
    let mut at = index.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

pub fn ceil_boundary(text: &str, index: usize) -> usize {
    let mut at = index.min(text.len());
    while !text.is_char_boundary(at) {
        at += 1;
    }
    at
}

pub fn split_near(text: &str, index: usize) -> (&str, &str) {
    text.split_at(floor_boundary(text, index))
}

fn main() {
    println!("{}", floor_boundary("привет", 3));
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: &str = "aй👋b";

    #[test]
    fn floor_moves_left_to_a_boundary() {
        assert_eq!(floor_boundary(S, 0), 0);
        assert_eq!(floor_boundary(S, 1), 1);
        assert_eq!(floor_boundary(S, 2), 1, "индекс 2 попал внутрь буквы й");
        assert_eq!(floor_boundary(S, 5), 3, "индекс 5 попал внутрь эмодзи");
        assert_eq!(floor_boundary(S, 7), 7);
    }

    #[test]
    fn ceil_moves_right_to_a_boundary() {
        assert_eq!(ceil_boundary(S, 2), 3);
        assert_eq!(ceil_boundary(S, 4), 7);
        assert_eq!(ceil_boundary(S, 3), 3);
        assert_eq!(ceil_boundary(S, 0), 0);
    }

    #[test]
    fn an_index_past_the_end_is_the_end() {
        assert_eq!(floor_boundary(S, 100), S.len());
        assert_eq!(ceil_boundary(S, 100), S.len());
        assert_eq!(floor_boundary("", 5), 0);
        assert_eq!(ceil_boundary("", 5), 0);
    }

    #[test]
    fn splitting_never_panics_and_loses_nothing() {
        for index in 0..=12 {
            let (head, tail) = split_near(S, index);
            assert_eq!(format!("{head}{tail}"), S, "после разреза по индексу {index} часть текста потерялась");
            assert!(head.len() <= index.min(S.len()));
        }
        assert_eq!(split_near("привет", 5), ("пр", "ивет"));
    }
}
