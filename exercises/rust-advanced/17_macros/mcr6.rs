// В модуле `util` живёт макрос `word_counts!`, считающий слова, и он работает у автора. Но в модуле
// `consumer` (и в любом чужом коде) он не собирается: «failed to resolve: use of undeclared type
// `HashMap`», «cannot find function `normalize`». Макрос раскрывается в месте вызова, поэтому все его
// пути читаются там, а не там, где макрос написан. В тестах есть модуль, где свои `HashMap` и `normalize`,
// другие по смыслу, и макрос обязан их не замечать.
// TODO: Почини макрос, чтобы он работал в любом модуле и в любом крейте.
pub mod util {
    pub fn normalize(word: &str) -> String {
        word.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase()
    }

    #[macro_export]
    macro_rules! word_counts {
        ($text:expr) => {{
            let mut counts = HashMap::new();
            for word in $text.split_whitespace() {
                let word = normalize(word);
                if !word.is_empty() {
                    *counts.entry(word).or_insert(0usize) += 1;
                }
            }
            counts
        }};
    }
}

pub mod consumer {
    pub fn top_word(text: &str) -> Option<(String, usize)> {
        let counts = crate::word_counts!(text);
        counts.into_iter().max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)))
    }
}

fn main() {
    println!("{:?}", consumer::top_word("a b a"));
}

#[cfg(test)]
mod tests {
    use super::*;

    mod hostile {
        #![allow(dead_code)]
        pub struct HashMap;

        pub fn normalize(_: &str) -> usize {
            0
        }

        pub fn count(text: &str) -> usize {
            let counts = crate::word_counts!(text);
            counts.len()
        }
    }

    #[test]
    fn counts_words_ignoring_case_and_punctuation() {
        assert_eq!(consumer::top_word("Rust, rust! Async; rust?"), Some(("rust".to_string(), 3)));
        assert_eq!(consumer::top_word("..."), None);
    }

    #[test]
    fn the_tie_goes_to_the_alphabetically_first_word() {
        assert_eq!(consumer::top_word("b a"), Some(("a".to_string(), 1)));
    }

    #[test]
    fn works_in_a_module_with_its_own_hashmap_and_normalize() {
        assert_eq!(hostile::count("one two two three"), 3, "макрос обязан пользоваться своими путями, а не тем, что оказалось рядом");
    }

    #[test]
    fn the_result_is_a_plain_std_hashmap() {
        let counts: std::collections::HashMap<String, usize> = crate::word_counts!("a b a");
        assert_eq!(counts["a"], 2);
    }
}
