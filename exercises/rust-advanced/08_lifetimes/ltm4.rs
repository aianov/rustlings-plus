// Настройки приходят одной строкой: `retries=3; mode = fast ;;oops; name=ann`. Пары разделены `;`, внутри
// пары `ключ=значение`, пробелы по краям не считаются. Сегменты без `=` и с пустым ключом пропускаются.
// Читатель `Settings<'a>` держит ссылку на строку и отдаёт ключи и значения кусками этой же строки,
// без копий, поэтому результаты должны пережить самого читателя: прочитал, выбросил парсер, а пары остались.
// `rest` показывает ещё не прочитанный хвост.
// TODO: Реализуй `next_pair` и `rest`. Подумай, откуда в сигнатурах берётся `'a`, а не время жизни `&mut self`.

pub struct Settings<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> Settings<'a> {
    pub fn new(src: &'a str) -> Settings<'a> {
        Settings { src, pos: 0 }
    }

    pub fn next_pair(&mut self) -> Option<(&'a str, &'a str)> {
        todo!("следующая пара ключ-значение")
    }

    pub fn rest(&self) -> &'a str {
        todo!("непрочитанный хвост")
    }
}

fn main() {
    let mut s = Settings::new("a=1;b=2");
    println!("{:?}", s.next_pair());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_pairs_skipping_garbage() {
        let mut s = Settings::new("retries=3; mode = fast ;;oops; name=ann");
        assert_eq!(s.next_pair(), Some(("retries", "3")));
        assert_eq!(s.next_pair(), Some(("mode", "fast")));
        assert_eq!(s.next_pair(), Some(("name", "ann")), "сегменты без = и пустые пропускаются");
        assert_eq!(s.next_pair(), None);
        assert_eq!(s.next_pair(), None, "после конца всегда None");
    }

    #[test]
    fn rest_is_the_unread_tail() {
        let mut s = Settings::new("a=1; b=2;c=3");
        assert_eq!(s.rest(), "a=1; b=2;c=3");
        s.next_pair();
        assert_eq!(s.rest(), " b=2;c=3");
        while s.next_pair().is_some() {}
        assert_eq!(s.rest(), "");
    }

    #[test]
    fn empty_keys_and_values() {
        let mut s = Settings::new("=x; k=; =;z=1");
        assert_eq!(s.next_pair(), Some(("k", "")), "пустое значение допустимо, пустой ключ нет");
        assert_eq!(s.next_pair(), Some(("z", "1")));
    }

    #[test]
    fn works_with_non_ascii_text() {
        let mut s = Settings::new("имя=Аня; город = Тверь");
        assert_eq!(s.next_pair(), Some(("имя", "Аня")));
        assert_eq!(s.next_pair(), Some(("город", "Тверь")));
    }

    #[test]
    fn pairs_outlive_the_reader() {
        let text = String::from("x=1;y=2");
        let pairs = {
            let mut reader = Settings::new(&text);
            let mut all = Vec::new();
            while let Some(p) = reader.next_pair() {
                all.push(p);
            }
            all
        };
        assert_eq!(pairs, vec![("x", "1"), ("y", "2")]);
        let start = text.as_ptr() as usize;
        assert!(pairs.iter().all(|(k, v)| (start..=start + text.len()).contains(&(k.as_ptr() as usize)) && (start..=start + text.len()).contains(&(v.as_ptr() as usize))), "ключи и значения режутся из исходной строки без копий");
    }
}
