// Когда клиент присылает кривой JSON, сервер должен ответить не «error», а так, чтобы разработчик
// клиента понял, что чинить. Ошибка `serde_json` говорит много, но в одной строке: тип ошибки (оборвано,
// не JSON, JSON не того вида), строка и столбец вперемешку с текстом. Для API нужно отдельное значение,
// по которому видно категорию, место и причину без хвоста «at line … column …».
// TODO: Реализуй `check`: разбери пост и преврати ошибку `serde_json` в `ParseReport`.
use serde::Deserialize;

#[derive(Debug, PartialEq, Deserialize)]
pub struct Post {
    pub id: u64,
    pub title: String,
}

#[derive(Debug, PartialEq)]
pub enum ParseReport {
    Truncated,
    Syntax { line: usize, column: usize },
    Shape { line: usize, column: usize, reason: String },
}

pub fn check(raw: &str) -> Result<Post, ParseReport> {
    todo!("разобрать и классифицировать ошибку")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn good_post() {
        assert_eq!(check(r#"{"id":1,"title":"hi"}"#), Ok(Post { id: 1, title: "hi".into() }));
    }

    #[test]
    fn cut_off_input_is_truncated() {
        assert_eq!(check(r#"{"id":"#), Err(ParseReport::Truncated));
        assert_eq!(check(r#"{"id":1,"title":"hi"#), Err(ParseReport::Truncated), "оборванная строка тоже обрыв");
        assert_eq!(check(""), Err(ParseReport::Truncated));
    }

    #[test]
    fn not_json_is_syntax_with_a_place() {
        assert_eq!(check(r#"{"id": x}"#), Err(ParseReport::Syntax { line: 1, column: 8 }));
        let multi = "{\n  \"id\": 1,\n  \"title\": @\n}";
        assert_eq!(check(multi), Err(ParseReport::Syntax { line: 3, column: 12 }), "строки считаются с единицы");
        assert!(matches!(check(r#"{"id":1,}"#), Err(ParseReport::Syntax { .. })));
    }

    #[test]
    fn valid_json_of_the_wrong_shape_has_a_clean_reason() {
        match check(r#"{"id":"a","title":"t"}"#) {
            Err(ParseReport::Shape { line, column, reason }) => {
                assert_eq!((line, column), (1, 9));
                assert_eq!(reason, "invalid type: string \"a\", expected u64", "причина без хвоста про строку и столбец");
            }
            other => panic!("ждали Shape, получили {other:?}"),
        }
        match check(r#"{"id":1}"#) {
            Err(ParseReport::Shape { reason, .. }) => assert_eq!(reason, "missing field `title`"),
            other => panic!("ждали Shape, получили {other:?}"),
        }
    }
}
