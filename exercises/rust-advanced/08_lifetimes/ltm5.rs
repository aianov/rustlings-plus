// Разбор строки запроса `/feed?page=3&author=ann`. `Query` держит кусок после `?` и отдаёт значения.
// Функция `page_of` берёт адрес, разбирает его и возвращает номер страницы, и не собирается:
// «cannot return value referencing local variable `query`», хотя значение лежит в самом адресе,
// а не в локальной переменной `query`. Дело в том, что метод `get` пообещал не то, что умеет.
// TODO: Измени сигнатуры методов `Query`, чтобы значения жили столько же, сколько исходный адрес.

pub struct Query<'a> {
    raw: &'a str,
}

impl<'a> Query<'a> {
    pub fn new(url: &'a str) -> Query<'a> {
        Query { raw: url.split_once('?').map_or("", |(_, q)| q) }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.raw.split('&').filter_map(|p| p.split_once('=')).find(|(k, _)| *k == key).map(|(_, v)| v)
    }

    pub fn keys(&self) -> Vec<&str> {
        self.raw.split('&').filter_map(|p| p.split_once('=')).map(|(k, _)| k).collect()
    }
}

pub fn page_of(url: &str) -> Option<&str> {
    let query = Query::new(url);
    query.get("page")
}

fn main() {
    println!("{:?}", page_of("/feed?page=3"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_values_by_key() {
        let q = Query::new("/feed?page=3&author=ann&page=4");
        assert_eq!(q.get("page"), Some("3"), "берётся первое вхождение");
        assert_eq!(q.get("author"), Some("ann"));
        assert_eq!(q.get("missing"), None);
        assert_eq!(q.keys(), vec!["page", "author", "page"]);
    }

    #[test]
    fn no_question_mark_means_no_pairs() {
        let q = Query::new("/feed");
        assert_eq!(q.get("page"), None);
        assert!(q.keys().is_empty());
    }

    #[test]
    fn page_of_returns_a_slice_of_the_url() {
        let url = String::from("/feed?page=12&x=1");
        let page = page_of(&url);
        assert_eq!(page, Some("12"));
        let start = url.as_ptr() as usize;
        assert!((start..start + url.len()).contains(&(page.unwrap().as_ptr() as usize)), "значение должно быть куском адреса, а не копией");
    }

    #[test]
    fn values_outlive_the_query_value() {
        let url = String::from("/a?k=v");
        let keys = {
            let q = Query::new(&url);
            q.keys()
        };
        assert_eq!(keys, vec!["k"]);
    }
}
