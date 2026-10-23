// Заголовки HTTP: имена не различают регистр ("content-type" и "Content-Type" одно и то же), одно имя
// может встретиться несколько раз ("Set-Cookie"), а порядок и исходный вид имён надо сохранить, чтобы
// вывести их обратно как пришли.
// TODO: Напиши структуру Headers с методами из тестов. set заменяет все значения одним, оставляя его
// на месте первого прежнего вхождения (если его не было, дописывает в конец).

#[derive(Debug, Default, Clone, PartialEq)]
struct Headers {
    // TODO: хранилище на твой вкус
}

impl Headers {
    fn new() -> Headers {
        Headers::default()
    }

    fn append(&mut self, name: &str, value: &str) {
        todo!()
    }

    fn get(&self, name: &str) -> Option<&str> {
        todo!()
    }

    fn get_all(&self, name: &str) -> Vec<&str> {
        todo!()
    }

    fn set(&mut self, name: &str, value: &str) {
        todo!()
    }

    fn remove(&mut self, name: &str) -> usize {
        todo!()
    }

    fn contains(&self, name: &str) -> bool {
        todo!()
    }

    fn len(&self) -> usize {
        todo!()
    }

    fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        std::iter::empty()
    }
}

fn main() {
    let mut h = Headers::new();
    h.append("Content-Type", "text/plain");
    println!("{:?} {}", h.get("content-type"), h.len());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Headers {
        let mut h = Headers::new();
        h.append("Host", "example.test");
        h.append("Set-Cookie", "a=1");
        h.append("Accept", "*/*");
        h.append("set-cookie", "b=2");
        h
    }

    #[test]
    fn lookup_ignores_case() {
        let h = sample();
        assert_eq!(h.get("host"), Some("example.test"));
        assert_eq!(h.get("HOST"), Some("example.test"));
        assert_eq!(h.get("missing"), None);
        assert!(h.contains("aCcEpT"));
        assert!(!h.contains("Accept-Encoding"), "Accept-Encoding это другое имя, а не начало Accept");
    }

    #[test]
    fn repeated_names_keep_all_values_in_order() {
        let h = sample();
        assert_eq!(h.get_all("set-cookie"), vec!["a=1", "b=2"]);
        assert_eq!(h.get("SET-COOKIE"), Some("a=1"), "get возвращает первое значение");
        assert_eq!(h.get_all("nothing"), Vec::<&str>::new());
        assert_eq!(h.len(), 4);
    }

    #[test]
    fn iteration_keeps_order_and_original_case() {
        let h = sample();
        let all: Vec<(&str, &str)> = h.iter().collect();
        assert_eq!(all, vec![("Host", "example.test"), ("Set-Cookie", "a=1"), ("Accept", "*/*"), ("set-cookie", "b=2")]);
    }

    #[test]
    fn set_replaces_in_place() {
        let mut h = sample();
        h.set("SET-COOKIE", "z=9");
        let all: Vec<(&str, &str)> = h.iter().collect();
        assert_eq!(all, vec![("Host", "example.test"), ("Set-Cookie", "z=9"), ("Accept", "*/*")], "set оставляет одно значение на месте первого вхождения");
        h.set("X-New", "1");
        assert_eq!(h.iter().last(), Some(("X-New", "1")), "нового имени ещё не было: дописывается в конец");
        assert_eq!(h.len(), 4);
    }

    #[test]
    fn remove_counts_what_it_removed() {
        let mut h = sample();
        assert_eq!(h.remove("SET-cookie"), 2);
        assert_eq!(h.remove("set-cookie"), 0);
        assert_eq!(h.len(), 2);
        assert_eq!(h.get("host"), Some("example.test"), "остальные заголовки остались");
    }
}
