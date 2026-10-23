// Валидатор принимает шаблон вида `*.example.com` или `news-*-2024*` (звёздочка это любая цепочка символов, вопросительный знак любой один
// символ) и сравнивает строку с шаблоном. Реализация простая и красивая: рекурсия, на звёздочке пробуем все варианты. Поставили на вход шаблон с
// двадцатью звёздочками и строку из трёх тысяч букв `a`: процессор занят вечность. Число путей разбора растёт экспоненциально, и один запрос
// кладёт сервис (ReDoS, отказ в обслуживании регулярным выражением).
// Цель: тот же результат на любых входах, но время растёт не быстрее, чем произведение длины шаблона на длину строки.


fn main() {}

pub fn glob_match(pattern: &str, text: &str) -> bool {
    fn rec(p: &[char], t: &[char]) -> bool {
        match p.first() {
            None => t.is_empty(),
            Some('*') => (0..=t.len()).any(|i| rec(&p[1..], &t[i..])),
            Some('?') => !t.is_empty() && rec(&p[1..], &t[1..]),
            Some(c) => t.first() == Some(c) && rec(&p[1..], &t[1..]),
        }
    }
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    rec(&p, &t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn it_matches_like_a_glob() {
        let table = [
            ("", "", true),
            ("", "a", false),
            ("*", "", true),
            ("*", "что угодно", true),
            ("a*", "abc", true),
            ("a*", "ba", false),
            ("*c", "abc", true),
            ("*.example.com", "www.example.com", true),
            ("*.example.com", "example.com", false),
            ("a?c", "abc", true),
            ("a?c", "ac", false),
            ("a?c", "abbc", false),
            ("news-*-2024*", "news-politics-2024-05", true),
            ("news-*-2024*", "news-2024", false),
            ("**", "x", true),
            ("a**b", "ab", true),
            ("?*", "", false),
            ("?*", "я", true),
            ("ё*я", "ёжик моя", true),
            ("*a*b*c*", "xxaxxbxxcxx", true),
            ("*a*b*c*", "xxaxxcxxbxx", false),
            ("abc", "abc", true),
            ("abc", "abcd", false),
            ("*abc", "ababc", true),
        ];
        for (pattern, text, expected) in table {
            assert_eq!(glob_match(pattern, text), expected, "glob_match({pattern:?}, {text:?})");
        }
    }

    fn within(limit: Duration, pattern: String, text: String) -> Option<bool> {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(glob_match(&pattern, &text));
        });
        rx.recv_timeout(limit).ok()
    }

    #[test]
    fn a_hostile_pattern_does_not_hang_the_service() {
        let pattern = format!("{}*b", "*a".repeat(20));
        let text = "a".repeat(3000);
        assert_eq!(within(Duration::from_secs(3), pattern, text), Some(false), "сопоставление не уложилось в 3 секунды: время растёт экспоненциально");
    }

    #[test]
    fn long_inputs_are_fine() {
        let text = "ab".repeat(50_000);
        assert_eq!(within(Duration::from_secs(3), "*ab*ba*ab*".to_string(), text.clone()), Some(true));
        assert_eq!(within(Duration::from_secs(3), format!("{}x", "?".repeat(50_000)), text), Some(false));
    }
}
