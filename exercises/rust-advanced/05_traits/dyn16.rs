// Преобразователь строк иногда хранит ссылку на внешние данные: например, префикс, который живёт
// в чужой строке. `Box<dyn Transform>` без пометок значит `Box<dyn Transform + 'static>`: объект не
// держит ссылок на что-то короткоживущее. Фабрика `build` возвращает преобразователь, который может
// ссылаться на префикс вызывающего, и компилятор не может доказать, что это безопасно.
// Режимы: `upper` делает ПРОПИСНЫМИ и дописывает префикс в начало, любое другое имя просто дописывает
// префикс.
// TODO: Напиши структуру-преобразователь, которая держит префикс, и реализуй для неё `Transform`,
// затем `build` и `apply_all`. Сигнатура `build` должна честно говорить, что результат не может пережить префикс.

pub trait Transform {
    fn apply(&self, input: &str) -> String;
}

pub struct Prefixer<'a> {
    pub prefix: &'a str,
    pub upper: bool,
}

pub fn build<'a>(mode: &str, prefix: &'a str) -> Box<dyn Transform> {
    todo!("Prefixer, upper = (mode == \"upper\")")
}

pub fn apply_all(transforms: &[Box<dyn Transform + '_>], input: &str) -> Vec<String> {
    todo!("результат каждого преобразователя")
}

fn main() {
    let prefix = String::from(">> ");
    println!("{}", build("upper", &prefix).apply("hi"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upper_and_plain() {
        let prefix = String::from("> ");
        assert_eq!(build("upper", &prefix).apply("hi"), "> HI");
        assert_eq!(build("plain", &prefix).apply("hi"), "> hi");
        assert_eq!(build("whatever", &prefix).apply("Hi"), "> Hi", "неизвестный режим просто префикс");
    }

    #[test]
    fn several_transforms_over_one_borrowed_prefix() {
        let prefix = String::from("# ");
        let list = vec![build("upper", &prefix), build("plain", &prefix)];
        assert_eq!(apply_all(&list, "go"), vec!["# GO", "# go"]);
        assert!(apply_all(&[], "x").is_empty());
    }

    #[test]
    fn owned_transform_still_fits_in_the_same_list() {
        struct Shout;
        impl Transform for Shout {
            fn apply(&self, input: &str) -> String {
                format!("{input}!")
            }
        }
        let prefix = String::from("p:");
        let list: Vec<Box<dyn Transform + '_>> = vec![build("plain", &prefix), Box::new(Shout)];
        assert_eq!(apply_all(&list, "a"), vec!["p:a", "a!"], "объекты с заимствованием и без него уживаются в одном списке");
    }
}
