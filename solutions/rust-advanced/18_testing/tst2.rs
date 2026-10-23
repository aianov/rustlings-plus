pub fn parse_port(text: &str) -> Option<u16> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse::<u16>().ok().filter(|&port| port != 0)
}

pub struct Case {
    pub name: &'static str,
    pub input: &'static str,
    pub expected: Option<u16>,
}

pub fn cases() -> Vec<Case> {
    vec![
        Case { name: "обычный порт", input: "8080", expected: Some(8080) },
        Case { name: "минимум", input: "1", expected: Some(1) },
        Case { name: "максимум", input: "65535", expected: Some(65535) },
        Case { name: "ноль", input: "0", expected: None },
        Case { name: "на единицу больше максимума", input: "65536", expected: None },
        Case { name: "сильное переполнение", input: "70000", expected: None },
        Case { name: "пустая строка", input: "", expected: None },
        Case { name: "пробел в начале", input: " 80", expected: None },
        Case { name: "пробел в конце", input: "80 ", expected: None },
        Case { name: "плюс", input: "+80", expected: None },
        Case { name: "буквы", input: "80a", expected: None },
        Case { name: "ведущие нули", input: "0080", expected: Some(80) },
    ]
}

pub fn run_table(parse: fn(&str) -> Option<u16>, cases: &[Case]) -> Result<(), String> {
    let failures: Vec<String> = cases
        .iter()
        .filter_map(|case| {
            let got = parse(case.input);
            (got != case.expected).then(|| format!("случай `{}`: вход {:?}, ждали {:?}, получили {:?}", case.name, case.input, case.expected, got))
        })
        .collect();
    if failures.is_empty() { Ok(()) } else { Err(failures.join("\n")) }
}

fn main() {
    match run_table(parse_port, &cases()) {
        Ok(()) => println!("все случаи прошли"),
        Err(report) => println!("{report}"),
    }
}

#[cfg(test)]
mod grading {
    use super::*;

    type Parser = fn(&str) -> Option<u16>;

    fn digits_only(text: &str) -> bool {
        !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit())
    }
    fn m_zero_allowed(text: &str) -> Option<u16> {
        if digits_only(text) { text.parse::<u16>().ok() } else { None }
    }
    fn m_off_by_one(text: &str) -> Option<u16> {
        parse_port(text).filter(|&p| p < 65535)
    }
    fn m_trims(text: &str) -> Option<u16> {
        parse_port(text.trim())
    }
    fn m_plus(text: &str) -> Option<u16> {
        text.parse::<u16>().ok().filter(|&p| p != 0)
    }
    fn m_empty_default(text: &str) -> Option<u16> {
        if text.is_empty() { Some(80) } else { parse_port(text) }
    }
    fn m_no_leading_zeros(text: &str) -> Option<u16> {
        if text.len() > 1 && text.starts_with('0') { None } else { parse_port(text) }
    }
    fn m_wraps(text: &str) -> Option<u16> {
        if digits_only(text) { text.parse::<u32>().ok().map(|p| p as u16).filter(|&p| p != 0) } else { None }
    }

    #[test]
    fn real_parser_passes_the_table() {
        let table = cases();
        assert!(table.len() >= 8, "в таблице меньше восьми случаев: мало, чтобы поймать типичные ошибки");
        if let Err(report) = run_table(parse_port, &table) {
            panic!("настоящий парсер не проходит твою таблицу (ошибка в ожиданиях?):\n{report}");
        }
    }

    #[test]
    fn names_are_unique_and_meaningful() {
        let table = cases();
        let mut names: Vec<&str> = table.iter().map(|c| c.name).collect();
        assert!(names.iter().all(|n| n.trim().len() >= 3), "у каждого случая должно быть осмысленное имя");
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), table.len(), "имена случаев должны быть уникальными, иначе по провалу не понять, какой упал");
    }

    #[test]
    fn table_catches_every_mutant() {
        let mutants: [(&str, Parser); 7] = [
            ("ноль допускается", m_zero_allowed),
            ("верхняя граница на единицу меньше", m_off_by_one),
            ("пробелы обрезаются", m_trims),
            ("плюс допускается", m_plus),
            ("пустая строка превращается в 80", m_empty_default),
            ("ведущие нули запрещены", m_no_leading_zeros),
            ("большие числа заворачиваются в u16", m_wraps),
        ];
        let table = cases();
        for (what, mutant) in mutants {
            let report = match run_table(mutant, &table) {
                Ok(()) => panic!("таблица не заметила ошибку «{what}»: добавь случай, на котором она видна"),
                Err(report) => report,
            };
            for case in table.iter().filter(|c| mutant(c.input) != c.expected) {
                assert!(report.contains(case.name), "ошибка «{what}»: в сообщении нет имени упавшего случая `{}`", case.name);
            }
            for case in table.iter().filter(|c| mutant(c.input) == c.expected) {
                assert!(!report.contains(case.name), "ошибка «{what}»: в сообщении лишний случай `{}`, он не падал", case.name);
            }
        }
    }

    #[test]
    fn report_shows_input_and_results() {
        let table = vec![Case { name: "пробел перед числом", input: " 80", expected: None }];
        let report = run_table(m_trims, &table).expect_err("m_trims должен упасть");
        assert!(report.contains("\" 80\""), "в сообщении нужен вход в кавычках через {{:?}}, чтобы пробел был виден: {report}");
        assert!(report.contains("None") && report.contains("Some(80)"), "в сообщении нужны и ожидаемое, и полученное значение: {report}");
    }
}
