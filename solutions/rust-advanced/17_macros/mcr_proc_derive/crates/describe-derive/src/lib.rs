use proc_macro::{Delimiter, TokenStream, TokenTree};

enum Shape {
    Named(Vec<String>),
    Tuple(usize),
    Unit,
    Enum(Vec<String>),
}

#[proc_macro_derive(Describe)]
pub fn derive_describe(input: TokenStream) -> TokenStream {
    match parse(input) {
        Ok((name, shape)) => generate(&name, &shape).parse().expect("derive Describe сгенерировал неверный код"),
        Err(message) => format!("compile_error!({message:?});").parse().expect("compile_error"),
    }
}

fn skip_attributes_and_visibility(tokens: &mut Vec<TokenTree>) {
    loop {
        match tokens.first() {
            Some(TokenTree::Punct(p)) if p.as_char() == '#' => {
                tokens.remove(0);
                if !tokens.is_empty() {
                    tokens.remove(0);
                }
            }
            Some(TokenTree::Ident(i)) if i.to_string() == "pub" => {
                tokens.remove(0);
                if let Some(TokenTree::Group(g)) = tokens.first() {
                    if g.delimiter() == Delimiter::Parenthesis {
                        tokens.remove(0);
                    }
                }
            }
            _ => break,
        }
    }
}

fn split_top_level(stream: TokenStream) -> Vec<Vec<TokenTree>> {
    let mut parts = Vec::new();
    let mut current = Vec::new();
    let mut depth = 0usize;
    let mut previous_minus = false;
    for token in stream {
        let mut is_minus = false;
        if let TokenTree::Punct(p) = &token {
            match p.as_char() {
                '<' => depth += 1,
                '>' if previous_minus => {}
                '>' => depth = depth.saturating_sub(1),
                ',' if depth == 0 => {
                    parts.push(std::mem::take(&mut current));
                    previous_minus = false;
                    continue;
                }
                '-' => is_minus = true,
                _ => {}
            }
        }
        previous_minus = is_minus;
        current.push(token);
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts.into_iter().filter(|part| !part.is_empty()).collect()
}

fn first_ident(mut tokens: Vec<TokenTree>) -> Result<String, String> {
    skip_attributes_and_visibility(&mut tokens);
    match tokens.first() {
        Some(TokenTree::Ident(i)) => Ok(i.to_string()),
        _ => Err("Describe: не удалось прочитать имя".to_string()),
    }
}

fn parse(input: TokenStream) -> Result<(String, Shape), String> {
    let mut tokens: Vec<TokenTree> = input.into_iter().collect();
    skip_attributes_and_visibility(&mut tokens);
    let mut tokens = tokens.into_iter();
    let kind = match tokens.next() {
        Some(TokenTree::Ident(i)) => i.to_string(),
        _ => return Err("Describe: ожидалось struct или enum".to_string()),
    };
    let name = match tokens.next() {
        Some(TokenTree::Ident(i)) => i.to_string(),
        _ => return Err("Describe: не найдено имя типа".to_string()),
    };
    let body = tokens.next();
    if let Some(TokenTree::Punct(p)) = &body {
        if p.as_char() == '<' {
            return Err("Describe: generic-типы не поддерживаются".to_string());
        }
    }
    match (kind.as_str(), body) {
        ("struct", Some(TokenTree::Group(group))) if group.delimiter() == Delimiter::Brace => {
            let fields = split_top_level(group.stream()).into_iter().map(first_ident).collect::<Result<Vec<_>, _>>()?;
            Ok((name, Shape::Named(fields)))
        }
        ("struct", Some(TokenTree::Group(group))) if group.delimiter() == Delimiter::Parenthesis => {
            Ok((name, Shape::Tuple(split_top_level(group.stream()).len())))
        }
        ("struct", _) => Ok((name, Shape::Unit)),
        ("enum", Some(TokenTree::Group(group))) if group.delimiter() == Delimiter::Brace => {
            let variants = split_top_level(group.stream()).into_iter().map(first_ident).collect::<Result<Vec<_>, _>>()?;
            Ok((name, Shape::Enum(variants)))
        }
        _ => Err("Describe работает только для struct и enum".to_string()),
    }
}

fn string_list(items: &[String]) -> String {
    items.iter().map(|item| format!("{item:?}")).collect::<Vec<_>>().join(", ")
}

fn generate(name: &str, shape: &Shape) -> String {
    match shape {
        Shape::Named(fields) => {
            let text = if fields.is_empty() { name.to_string() } else { format!("{name} {{ {} }}", fields.join(", ")) };
            format!(
                "impl {name} {{ pub const TYPE_NAME: &'static str = {name:?}; pub const FIELDS: &'static [&'static str] = &[{}]; pub fn describe() -> String {{ {text:?}.to_string() }} }}",
                string_list(fields)
            )
        }
        Shape::Tuple(count) => {
            let fields: Vec<String> = (0..*count).map(|i| i.to_string()).collect();
            let text = format!("{name}({})", fields.join(", "));
            format!(
                "impl {name} {{ pub const TYPE_NAME: &'static str = {name:?}; pub const FIELDS: &'static [&'static str] = &[{}]; pub fn describe() -> String {{ {text:?}.to_string() }} }}",
                string_list(&fields)
            )
        }
        Shape::Unit => format!(
            "impl {name} {{ pub const TYPE_NAME: &'static str = {name:?}; pub const FIELDS: &'static [&'static str] = &[]; pub fn describe() -> String {{ {name:?}.to_string() }} }}"
        ),
        Shape::Enum(variants) => {
            let text = format!("{name} = {}", variants.join(" | "));
            format!(
                "impl {name} {{ pub const TYPE_NAME: &'static str = {name:?}; pub const VARIANTS: &'static [&'static str] = &[{}]; pub fn describe() -> String {{ {text:?}.to_string() }} }}",
                string_list(variants)
            )
        }
    }
}
