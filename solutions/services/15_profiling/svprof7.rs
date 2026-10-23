
use std::collections::HashMap;

fn main() {}

#[derive(Debug, PartialEq)]
pub struct ParseError {
    pub line: usize,
    pub reason: String,
}

pub struct Profile {
    stacks: Vec<(Vec<String>, u64)>,
}

impl Profile {
    pub fn parse(text: &str) -> Result<Profile, ParseError> {
        let mut stacks = Vec::new();
        for (i, raw) in text.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() {
                continue;
            }
            let (frames, count) = line.rsplit_once(' ').ok_or(ParseError { line: i + 1, reason: "нет числа замеров в конце строки".to_string() })?;
            let count: u64 = count.parse().map_err(|_| ParseError { line: i + 1, reason: format!("«{count}» не число замеров") })?;
            let frames: Vec<String> = frames.split(';').map(str::to_string).collect();
            if frames.iter().any(|f| f.is_empty()) {
                return Err(ParseError { line: i + 1, reason: "пустое имя функции в стеке".to_string() });
            }
            stacks.push((frames, count));
        }
        Ok(Profile { stacks })
    }

    pub fn total_samples(&self) -> u64 {
        self.stacks.iter().map(|(_, n)| n).sum()
    }

    pub fn self_time(&self, name: &str) -> u64 {
        self.stacks.iter().filter(|(frames, _)| frames.last().map(String::as_str) == Some(name)).map(|(_, n)| n).sum()
    }

    pub fn total_time(&self, name: &str) -> u64 {
        self.stacks.iter().filter(|(frames, _)| frames.iter().any(|f| f == name)).map(|(_, n)| n).sum()
    }

    fn ranked(&self, value: impl Fn(&str) -> u64, n: usize) -> Vec<(String, u64)> {
        let mut names: Vec<&str> = self.stacks.iter().flat_map(|(frames, _)| frames.iter().map(String::as_str)).collect();
        names.sort_unstable();
        names.dedup();
        let mut ranked: Vec<(String, u64)> = names.into_iter().map(|name| (name.to_string(), value(name))).filter(|(_, v)| *v > 0).collect();
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        ranked.truncate(n);
        ranked
    }

    pub fn top_self(&self, n: usize) -> Vec<(String, u64)> {
        self.ranked(|name| self.self_time(name), n)
    }

    pub fn top_total(&self, n: usize) -> Vec<(String, u64)> {
        self.ranked(|name| self.total_time(name), n)
    }

    pub fn by_name(&self) -> HashMap<String, (u64, u64)> {
        let mut all = HashMap::new();
        for (frames, _) in &self.stacks {
            for f in frames {
                all.entry(f.clone()).or_insert_with(|| (self.self_time(f), self.total_time(f)));
            }
        }
        all
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
main;server;handle_request;load 20
main;server;handle_request;render;serialize;escape 300
main;server;handle_request;render;serialize 100
main;server;handle_request;render 50
main;server;accept 30
main;work;fib;fib;fib;fib 40
";

    fn profile() -> Profile {
        Profile::parse(SAMPLE).expect("образец разбирается")
    }

    fn pair(name: &str, n: u64) -> (String, u64) {
        (name.to_string(), n)
    }

    #[test]
    fn total_time_counts_the_function_anywhere_in_the_stack() {
        let p = profile();
        assert_eq!(p.total_samples(), 540);
        assert_eq!(p.total_time("handle_request"), 470, "handle_request есть в первых четырёх стеках: 20 + 300 + 100 + 50");
        assert_eq!(p.total_time("render"), 450);
        assert_eq!(p.total_time("main"), 540);
        assert_eq!(p.total_time("nobody"), 0);
    }

    #[test]
    fn self_time_counts_only_the_top_of_the_stack() {
        let p = profile();
        assert_eq!(p.self_time("handle_request"), 0, "сама handle_request ни одного замера не оказалась на вершине: она только вызывает");
        assert_eq!(p.self_time("escape"), 300);
        assert_eq!(p.self_time("serialize"), 100, "serialize на вершине в одном стеке, в другом она лишь вызывает escape");
        assert_eq!(p.self_time("render"), 50);
    }

    #[test]
    fn recursion_is_not_counted_twice() {
        let p = profile();
        assert_eq!(p.total_time("fib"), 40, "в стеке fib встречается четыре раза, но замеров всего 40");
        assert_eq!(p.self_time("fib"), 40);
    }

    #[test]
    fn the_hot_spot_is_found_by_self_time_not_by_total_time() {
        let p = profile();
        assert_eq!(p.top_self(3), vec![pair("escape", 300), pair("serialize", 100), pair("render", 50)], "горячая функция escape, а не handle_request");
        assert_eq!(p.top_total(3), vec![pair("main", 540), pair("server", 500), pair("handle_request", 470)], "по полному времени сверху те, кто только вызывает");
    }

    #[test]
    fn ties_are_broken_by_name_and_n_limits_the_list() {
        let p = Profile::parse("a;x 5\na;y 5\na;z 7\n").expect("разбирается");
        assert_eq!(p.top_self(10), vec![pair("z", 7), pair("x", 5), pair("y", 5)], "по убыванию, при равенстве по имени");
        assert_eq!(p.top_self(1), vec![pair("z", 7)]);
        assert!(p.top_self(0).is_empty());
    }

    #[test]
    fn names_with_spaces_and_blank_lines_are_fine() {
        let p = Profile::parse("\nmain;<impl Fn for Closure> 12\n\n  main;other 3  \n").expect("разбирается");
        assert_eq!(p.self_time("<impl Fn for Closure>"), 12, "число замеров отделяется по последнему пробелу");
        assert_eq!(p.total_samples(), 15);
    }

    #[test]
    fn broken_lines_are_reported_with_their_number() {
        let err = Profile::parse("a;b 3\nc;d\n").err().expect("вторая строка без числа");
        assert_eq!(err.line, 2);
        let err = Profile::parse("a;b x\n").err().expect("не число");
        assert_eq!((err.line, err.reason.contains('x')), (1, true), "в причине называется плохое значение: {}", err.reason);
        let err = Profile::parse("ok 1\na;;b 2\n").err().expect("пустое имя");
        assert_eq!(err.line, 2);
    }

    #[test]
    fn the_per_function_table_agrees_with_the_queries() {
        let table = profile().by_name();
        assert_eq!(table["handle_request"], (0, 470));
        assert_eq!(table["escape"], (300, 300));
        assert_eq!(table["fib"], (40, 40));
    }
}
