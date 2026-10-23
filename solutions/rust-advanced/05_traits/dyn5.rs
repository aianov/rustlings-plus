pub trait Plugin {
    fn name(&self) -> String;
    fn new() -> Self
    where
        Self: Sized;
    fn duplicate(&self) -> Box<dyn Plugin>;
    fn describe_with<T: std::fmt::Display>(&self, extra: T) -> String
    where
        Self: Sized;
}

pub struct Echo;
pub struct Reverse;

impl Plugin for Echo {
    fn name(&self) -> String {
        "echo".to_string()
    }

    fn new() -> Self {
        Echo
    }

    fn duplicate(&self) -> Box<dyn Plugin> {
        Box::new(Echo)
    }

    fn describe_with<T: std::fmt::Display>(&self, extra: T) -> String {
        format!("echo({extra})")
    }
}

impl Plugin for Reverse {
    fn name(&self) -> String {
        "reverse".to_string()
    }

    fn new() -> Self {
        Reverse
    }

    fn duplicate(&self) -> Box<dyn Plugin> {
        Box::new(Reverse)
    }

    fn describe_with<T: std::fmt::Display>(&self, extra: T) -> String {
        format!("reverse({extra})")
    }
}

pub fn names(plugins: &[Box<dyn Plugin>]) -> Vec<String> {
    plugins.iter().map(|p| p.name()).collect()
}

pub fn copies(plugins: &[Box<dyn Plugin>]) -> Vec<Box<dyn Plugin>> {
    plugins.iter().map(|p| p.duplicate()).collect()
}

fn main() {
    let list: Vec<Box<dyn Plugin>> = vec![Box::new(Echo::new())];
    println!("{:?}", names(&list));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugins_live_in_one_list() {
        let list: Vec<Box<dyn Plugin>> = vec![Box::new(Echo), Box::new(Reverse)];
        assert_eq!(names(&list), vec!["echo", "reverse"]);
    }

    #[test]
    fn list_can_be_copied() {
        let list: Vec<Box<dyn Plugin>> = vec![Box::new(Reverse), Box::new(Echo)];
        let copy = copies(&list);
        assert_eq!(names(&copy), vec!["reverse", "echo"], "копия содержит те же плагины в том же порядке");
    }

    #[test]
    fn concrete_types_keep_their_extras() {
        let e = Echo::new();
        assert_eq!(e.describe_with(5), "echo(5)", "generic-метод остаётся доступным на конкретном типе");
        assert_eq!(Reverse::new().describe_with("x"), "reverse(x)");
        assert_eq!(e.duplicate().name(), "echo");
    }
}
