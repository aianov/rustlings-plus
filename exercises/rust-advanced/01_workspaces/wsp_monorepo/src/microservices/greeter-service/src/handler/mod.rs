pub fn greeting(text: &str) -> String {
    let names = text_engine::mentions(text);
    if names.is_empty() {
        "greeting: nobody".to_string()
    } else {
        format!("greeting: {}", names.join(", "))
    }
}
