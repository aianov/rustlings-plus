// Браузер присылает все cookie одной строкой: "theme=dark; sid=abc123". Из неё нужно достать пары, не споткнувшись
// о значения с '=' внутри (токены в base64 так и выглядят), о пары без значения и о повторы одного имени. Одна
// испорченная пара не должна выбрасывать остальные: сессию пользователя терять из-за чужого мусора нельзя.
// TODO: Напиши parse_cookies (все пары по порядку) и cookie (первое значение с этим именем). Значение может
// быть в двойных кавычках (парные снимаются). Декодирования нет, запятая не разделитель.

fn parse_cookies(header: &str) -> Vec<(String, String)> {
    let _ = header;
    todo!()
}

fn cookie<'a>(pairs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    let _ = (pairs, name);
    todo!()
}

fn main() {
    let pairs = parse_cookies("theme=dark; sid=abc123");
    println!("{pairs:?} {:?}", cookie(&pairs, "sid"));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(header: &str) -> Vec<(&'static str, &'static str)> {
        parse_cookies(header).into_iter().map(|(n, v)| (leak(n), leak(v))).collect()
    }

    fn leak(s: String) -> &'static str {
        Box::leak(s.into_boxed_str())
    }

    #[test]
    fn two_pairs() {
        assert_eq!(pairs("theme=dark; sid=abc123"), [("theme", "dark"), ("sid", "abc123")]);
        assert_eq!(pairs("one=1"), [("one", "1")]);
    }

    #[test]
    fn spaces_are_optional_and_extra_separators_are_ignored() {
        assert_eq!(pairs("a=1;b=2"), [("a", "1"), ("b", "2")], "после ';' пробела может не быть");
        assert_eq!(pairs("  a = 1 ;;  b=2;"), [("a", "1"), ("b", "2")], "пустые пары и лишние пробелы не мешают");
        assert_eq!(pairs(""), []);
    }

    #[test]
    fn value_may_contain_equals_and_may_be_empty() {
        assert_eq!(pairs("token=eyJhbGci==; empty="), [("token", "eyJhbGci=="), ("empty", "")], "'=' внутри значения (base64) остаётся в значении");
    }

    #[test]
    fn quotes_are_stripped_once() {
        assert_eq!(pairs("a=\"hello world\"; b=\"\"; c=\"x"), [("a", "hello world"), ("b", ""), ("c", "\"x")], "парные кавычки снимаются, одинокая остаётся");
    }

    #[test]
    fn junk_pairs_are_skipped_not_fatal() {
        assert_eq!(pairs("novalue; =nameless; good=1; bad name=2"), [("good", "1")], "по порче одной пары остальные не теряются");
    }

    #[test]
    fn nothing_is_decoded_or_split_further() {
        assert_eq!(pairs("a=%41b; list=1,2,3"), [("a", "%41b"), ("list", "1,2,3")], "cookie не URL-кодируются и запятая в значении не разделитель");
    }

    #[test]
    fn duplicates_are_kept_and_first_wins() {
        let all = parse_cookies("sid=first; other=x; sid=second");
        assert_eq!(all.len(), 3, "повторы не склеиваются и не теряются");
        assert_eq!(cookie(&all, "sid"), Some("first"), "браузер шлёт самые специфичные cookie первыми");
        assert_eq!(cookie(&all, "other"), Some("x"));
        assert_eq!(cookie(&all, "missing"), None);
        assert_eq!(cookie(&all, "SID"), None, "имена cookie чувствительны к регистру");
    }
}
