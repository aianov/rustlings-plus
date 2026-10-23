// Тест на страницу «популярные теги» то зелёный, то красный: у `HashMap` порядок обхода зависит от случайного
// зерна, и каждый запуск процесса он другой. Пользователям это тоже видно: теги прыгают местами.
// TODO: Сделай оба вывода детерминированными. Теги: по убыванию числа постов, при равенстве по алфавиту.
// Множество id отдаётся по возрастанию.
use std::collections::{HashMap, HashSet};

pub fn render_tags(tags: &HashMap<String, u32>) -> String {
    let mut parts = Vec::new();
    for (tag, n) in tags {
        parts.push(format!("{tag}:{n}"));
    }
    parts.join(",")
}

pub fn render_ids(ids: &HashSet<u64>) -> String {
    let parts: Vec<String> = ids.iter().map(|id| id.to_string()).collect();
    format!("[{}]", parts.join(","))
}

fn main() {
    let tags = HashMap::from([("rust".to_string(), 3)]);
    println!("{}", render_tags(&tags));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn many() -> Vec<(String, u32)> {
        (0..30).map(|i| (format!("tag{i:02}"), if i % 3 == 0 { 5 } else { 2 })).collect()
    }

    #[test]
    fn tags_are_ordered_by_count_then_name() {
        let tags: HashMap<String, u32> = many().into_iter().collect();
        let out = render_tags(&tags);
        let head = "tag00:5,tag03:5,tag06:5,tag09:5,tag12:5,tag15:5,tag18:5,tag21:5,tag24:5,tag27:5,tag01:2,tag02:2";
        assert!(out.starts_with(head), "ожидали сначала теги с 5 постами по алфавиту, потом с 2, получили {out}");
        assert_eq!(out.split(',').count(), 30);
    }

    #[test]
    fn the_output_does_not_depend_on_insertion_order() {
        let forward: HashMap<String, u32> = many().into_iter().collect();
        let backward: HashMap<String, u32> = many().into_iter().rev().collect();
        assert_eq!(render_tags(&forward), render_tags(&backward), "порядок вставки не должен влиять на вывод");
    }

    #[test]
    fn ids_are_ascending() {
        let ids: HashSet<u64> = (0..40).map(|i| (i * 7919) % 101).collect();
        let out = render_ids(&ids);
        let mut expected: Vec<u64> = ids.iter().copied().collect();
        expected.sort_unstable();
        let expected: Vec<String> = expected.iter().map(u64::to_string).collect();
        assert_eq!(out, format!("[{}]", expected.join(",")), "id должны идти по возрастанию");
        assert_eq!(render_ids(&HashSet::new()), "[]");
    }
}
