#![allow(unused_macros)]

macro_rules! strings {
    ($($s:expr),* $(,)?) => {
        vec![$($s.to_string()),*]
    };
}

macro_rules! total {
    ($($n:expr),* $(,)?) => {
        0 $(+ $n)*
    };
}

macro_rules! tag {
    ($name:ident $(, class = $c:expr)?) => {{
        let mut out = format!("<{}", stringify!($name));
        $(out.push_str(&format!(" class=\"{}\"", $c));)?
        out.push('>');
        out
    }};
}

fn main() {
    println!("{:?}", strings!["a", "b"]);
}

#[cfg(test)]
mod tests {
    #[test]
    fn strings_accept_empty_and_trailing_comma() {
        let empty: Vec<String> = strings!();
        assert!(empty.is_empty());
        assert_eq!(strings!["a", 1, 2.5], vec!["a", "1", "2.5"]);
        assert_eq!(strings!["x", "y",], vec!["x", "y"]);
    }

    #[test]
    fn total_handles_zero_one_and_many() {
        let nothing: u64 = total!();
        assert_eq!(nothing, 0);
        assert_eq!(total!(5), 5);
        assert_eq!(total!(1, 2, 3,), 6);
        let big: u64 = total!(10_u64.pow(10), 1);
        assert_eq!(big, 10_000_000_001);
    }

    #[test]
    fn total_keeps_the_expression_structure() {
        assert_eq!(total!(2 * 3, 4 - 1), 9);
    }

    #[test]
    fn tag_has_an_optional_class() {
        assert_eq!(tag!(div), "<div>");
        assert_eq!(tag!(p, class = "lead"), "<p class=\"lead\">");
        let c = String::from("a b");
        assert_eq!(tag!(span, class = c), "<span class=\"a b\">");
    }
}
