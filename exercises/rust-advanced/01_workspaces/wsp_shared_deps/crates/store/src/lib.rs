use model::Event;

pub fn encode(events: &[Event]) -> String {
    serde_json::to_string(events).expect("события всегда кодируются")
}

pub fn decode(text: &str) -> Result<Vec<Event>, serde_json::Error> {
    serde_json::from_str(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let events = vec![Event::new(1, "login"), Event::new(2, "logout")];
        assert_eq!(decode(&encode(&events)).expect("разбор"), events);
    }
}
