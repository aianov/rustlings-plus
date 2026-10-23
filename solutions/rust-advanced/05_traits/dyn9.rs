pub trait Named {
    fn name(&self) -> String;
}

pub trait Animal: Named {
    fn sound(&self) -> String;
}

pub struct Dog;
pub struct Cat {
    pub name: String,
}

impl Named for Dog {
    fn name(&self) -> String {
        "rex".to_string()
    }
}

impl Animal for Dog {
    fn sound(&self) -> String {
        "woof".to_string()
    }
}

impl Named for Cat {
    fn name(&self) -> String {
        self.name.clone()
    }
}

impl Animal for Cat {
    fn sound(&self) -> String {
        "meow".to_string()
    }
}

pub fn shout_name(n: &dyn Named) -> String {
    n.name().to_uppercase()
}

pub fn roll_call(animals: &[Box<dyn Animal>]) -> Vec<String> {
    animals.iter().map(|a| format!("{} says {}", shout_name(a.as_ref()), a.sound())).collect()
}

fn main() {
    println!("{}", shout_name(&Dog));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roll_call_uses_shout_name() {
        let text = include_str!("dyn9.rs");
        let code = text.split("#[cfg(test)]").next().unwrap_or("");
        assert!(code.matches("shout_name(").count() >= 2, "перекличка должна звать shout_name, а не делать to_uppercase сама");
        assert_eq!(code.matches("to_uppercase").count(), 1, "to_uppercase остаётся единственным, внутри shout_name");
    }

    #[test]
    fn every_animal_is_named() {
        assert_eq!(Dog.name(), "rex");
        assert_eq!(Cat { name: "tom".into() }.name(), "tom");
        assert_eq!(Dog.sound(), "woof");
    }

    #[test]
    fn named_works_on_upcast_objects() {
        let animal: Box<dyn Animal> = Box::new(Cat { name: "kit".into() });
        let as_named: &dyn Named = animal.as_ref();
        assert_eq!(shout_name(as_named), "KIT", "&dyn Animal приводится к &dyn Named");
    }

    #[test]
    fn roll_call_in_order() {
        let zoo: Vec<Box<dyn Animal>> = vec![Box::new(Dog), Box::new(Cat { name: "tom".into() })];
        assert_eq!(roll_call(&zoo), vec!["REX says woof", "TOM says meow"]);
        assert!(roll_call(&[]).is_empty());
    }
}
