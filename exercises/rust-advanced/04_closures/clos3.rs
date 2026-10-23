// Сканер слов подсчитывает совпадения и «кричит»: каждое найденное слово добавляется в верхнем регистре,
// а начиная со второго совпадения после каждого слова ещё и восклицательный знак. Счётчик увеличивает
// маленькое замыкание `hit`, но компилятор не даёт прочитать счётчик, пока замыкание живо.
// Пример: слова a x a y a и образец a дают (3, "AA!!A!").
// TODO: Заставь `scan` собраться, сохранив поведение. Подумай, кому на самом деле нужно замыкание.

pub fn scan(words: &[&str], needle: &str) -> (usize, String) {
    let mut count = 0;
    let mut hit = |w: &str| {
        count += 1;
        w.to_uppercase()
    };
    let mut shout = String::new();
    for w in words {
        if *w == needle {
            shout.push_str(&hit(w));
        }
        if count >= 2 {
            shout.push('!');
        }
    }
    (count, shout)
}

fn main() {
    println!("{:?}", scan(&["a", "b", "a"], "a"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_and_shouts() {
        assert_eq!(scan(&["a", "x", "a", "y", "a"], "a"), (3, "AA!!A!".to_string()));
    }

    #[test]
    fn no_matches() {
        assert_eq!(scan(&["x", "y"], "a"), (0, String::new()), "без совпадений тишина");
        assert_eq!(scan(&[], "a"), (0, String::new()));
    }

    #[test]
    fn single_match_has_no_exclamation() {
        assert_eq!(scan(&["x", "go", "y"], "go"), (1, "GO".to_string()), "восклицание только со второго совпадения");
    }

    #[test]
    fn exclamations_follow_every_later_word() {
        assert_eq!(scan(&["q", "q", "z", "z"], "q"), (2, "QQ!!!".to_string()), "после второго совпадения знак идёт и за чужими словами");
    }
}
