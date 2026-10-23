/// Одна запись таблицы маршрутов сервиса.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Route {
    pub name: &'static str,
    pub request: &'static str,
    pub response: &'static str,
}

/// Имена, которые встречаются в списке больше одного раза.
pub fn find_duplicates(names: &[&'static str]) -> Vec<&'static str> {
    let mut seen: Vec<&'static str> = Vec::new();
    let mut duplicates: Vec<&'static str> = Vec::new();
    for name in names {
        if seen.contains(name) {
            if !duplicates.contains(name) {
                duplicates.push(name);
            }
        } else {
            seen.push(name);
        }
    }
    duplicates
}

#[macro_export]
macro_rules! service {
    () => {};
}
