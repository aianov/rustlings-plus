pub trait Validator {
    fn check(&self, input: &str) -> Result<(), String>;
}

pub struct MaxLen(pub usize);

pub struct Banned<'a> {
    pub word: &'a str,
}

impl Validator for MaxLen {
    fn check(&self, input: &str) -> Result<(), String> {
        if input.chars().count() > self.0 {
            Err(format!("longer than {}", self.0))
        } else {
            Ok(())
        }
    }
}

impl Validator for Banned<'_> {
    fn check(&self, input: &str) -> Result<(), String> {
        if input.contains(self.word) {
            Err(format!("contains {}", self.word))
        } else {
            Ok(())
        }
    }
}

pub struct Registry<'a> {
    validators: Vec<Box<dyn Validator + 'a>>,
}

impl Default for Registry<'_> {
    fn default() -> Self {
        Registry::new()
    }
}

impl<'a> Registry<'a> {
    pub fn new() -> Registry<'a> {
        Registry { validators: Vec::new() }
    }

    pub fn add(&mut self, validator: impl Validator + 'a) {
        self.validators.push(Box::new(validator));
    }

    pub fn validate(&self, input: &str) -> Vec<String> {
        self.validators.iter().filter_map(|v| v.check(input).err()).collect()
    }
}

fn main() {
    let word = String::from("spam");
    let mut r = Registry::new();
    r.add(Banned { word: &word });
    println!("{:?}", r.validate("spam!"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owned_validators_work() {
        let mut r = Registry::new();
        r.add(MaxLen(3));
        assert!(r.validate("abc").is_empty());
        assert_eq!(r.validate("abcd"), vec!["longer than 3"]);
    }

    #[test]
    fn borrowing_validators_work() {
        let word = String::from("spam");
        let mut r = Registry::new();
        r.add(Banned { word: &word });
        r.add(MaxLen(10));
        assert_eq!(r.validate("no spam here"), vec!["contains spam", "longer than 10"], "ошибки идут в порядке добавления проверок");
        assert!(r.validate("ok").is_empty());
    }

    #[test]
    fn registry_can_be_filled_in_a_helper_with_the_same_lifetime() {
        fn fill<'a>(r: &mut Registry<'a>, banned: &'a str) {
            r.add(Banned { word: banned });
        }
        let word = String::from("bad");
        let mut r = Registry::new();
        fill(&mut r, &word);
        assert_eq!(r.validate("so bad"), vec!["contains bad"]);
    }
}
