pub fn longest(words: &[String]) -> Option<&str> {
    words.iter().map(String::as_str).max_by_key(|w| w.len())
}

pub fn contains_word(words: &[String], word: &str) -> bool {
    words.iter().any(|w| w == word)
}

pub fn first_n<T: Clone>(items: &[T], n: usize) -> Vec<T> {
    items.iter().take(n).cloned().collect()
}

pub fn shout(text: &str) -> String {
    text.to_uppercase()
}

fn main() {
    let words = vec!["a".to_string(), "bcd".to_string()];
    println!("{:?}", longest(&words));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words() -> Vec<String> {
        ["kiwi", "banana", "fig"].iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn longest_works_on_vec_slice_and_array() {
        let w = words();
        assert_eq!(longest(&w), Some("banana"));
        assert_eq!(longest(&w[2..]), Some("fig"), "срез тоже подходит");
        assert_eq!(longest(&["ab".to_string(), "c".to_string()]), Some("ab"), "и массив");
        assert_eq!(longest(&[]), None);
    }

    #[test]
    fn contains_word_takes_a_plain_str() {
        let w = words();
        assert!(contains_word(&w, "kiwi"), "литерал без .to_string()");
        assert!(contains_word(&w, &String::from("fig")), "&String приводится к &str сам");
        assert!(!contains_word(&w[..1], "banana"));
    }

    #[test]
    fn first_n_works_on_slices_of_anything() {
        assert_eq!(first_n(&[1, 2, 3, 4], 2), vec![1, 2]);
        let v = vec!["a".to_string(), "b".to_string()];
        assert_eq!(first_n(&v, 5), v, "n больше длины: вернуть всё");
        assert_eq!(first_n(&v[1..], 1), vec!["b".to_string()]);
    }

    #[test]
    fn shout_takes_str_and_string() {
        assert_eq!(shout("hi"), "HI");
        assert_eq!(shout(&String::from("ok")), "OK");
    }
}
