// Реестр проверок хранит валидаторы как `Box<dyn Validator>`. Один из валидаторов берёт запрещённое
// слово из строки, которая живёт в локальной переменной вызывающего. Реестр собирается, пока все валидаторы
// владеют своими данными, но стоит положить в него `Banned`, который держит `&str`, и компилятор
// сообщает про время жизни. Нужно сделать реестр способным хранить заимствующие объекты и при этом
// не позволить им пережить строки, на которые они ссылаются.
// TODO: Почини `Registry` и его методы. Тесты кладут в один реестр и владеющие, и заимствующие проверки.

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

pub struct Registry {
    validators: Vec<Box<dyn Validator>>,
}

impl Registry {
    pub fn new() -> Registry {
        Registry { validators: Vec::new() }
    }

    pub fn add(&mut self, validator: impl Validator + 'static) {
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
