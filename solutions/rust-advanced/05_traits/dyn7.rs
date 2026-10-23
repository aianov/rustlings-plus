pub trait Parser {
    type Output;
    fn parse(&self, input: &str) -> Option<Self::Output>;
}

pub struct IntParser;
pub struct BoolParser;
pub struct ListParser<P>(pub P);

impl Parser for IntParser {
    type Output = i64;

    fn parse(&self, input: &str) -> Option<i64> {
        input.trim().parse().ok()
    }
}

impl Parser for BoolParser {
    type Output = bool;

    fn parse(&self, input: &str) -> Option<bool> {
        match input {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        }
    }
}

impl<P: Parser> Parser for ListParser<P> {
    type Output = Vec<P::Output>;

    fn parse(&self, input: &str) -> Option<Self::Output> {
        if input.trim().is_empty() {
            return Some(Vec::new());
        }
        input.split(',').map(|piece| self.0.parse(piece)).collect()
    }
}

pub fn parse_all<P: Parser>(parser: &P, inputs: &[&str]) -> Vec<P::Output> {
    inputs.iter().filter_map(|input| parser.parse(input)).collect()
}

fn main() {
    println!("{:?}", IntParser.parse("42"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ints() {
        assert_eq!(IntParser.parse(" 42 "), Some(42));
        assert_eq!(IntParser.parse("-7"), Some(-7));
        assert_eq!(IntParser.parse("x"), None);
        assert_eq!(IntParser.parse(""), None);
    }

    #[test]
    fn bools_are_strict() {
        assert_eq!(BoolParser.parse("true"), Some(true));
        assert_eq!(BoolParser.parse("false"), Some(false));
        assert_eq!(BoolParser.parse("True"), None, "регистр важен");
        assert_eq!(BoolParser.parse(" true"), None, "пробелы не обрезаются");
    }

    #[test]
    fn lists_nest_and_fail_as_a_whole() {
        let ints = ListParser(IntParser);
        assert_eq!(ints.parse("1, 2,3"), Some(vec![1, 2, 3]));
        assert_eq!(ints.parse("1,x,3"), None, "один плохой кусок портит весь список");
        assert_eq!(ints.parse(""), Some(vec![]), "пустая строка это пустой список");
        let bools = ListParser(BoolParser);
        assert_eq!(bools.parse("true,false"), Some(vec![true, false]));
    }

    #[test]
    fn output_type_follows_the_parser() {
        let parsed: Option<Vec<i64>> = ListParser(IntParser).parse("5,6");
        assert_eq!(parsed.map(|v| v.iter().sum::<i64>()), Some(11));
        let all: Vec<bool> = parse_all(&BoolParser, &["true", "maybe", "false"]);
        assert_eq!(all, vec![true, false], "в parse_all остаются только разобранные входы");
        let lists: Vec<Vec<i64>> = parse_all(&ListParser(IntParser), &["1,2", "oops", "3"]);
        assert_eq!(lists, vec![vec![1, 2], vec![3]]);
    }
}
