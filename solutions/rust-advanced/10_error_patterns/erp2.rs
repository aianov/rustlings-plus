use std::cell::Cell;
use std::collections::HashMap;
use std::num::ParseIntError;

#[derive(Debug, PartialEq)]
pub struct NoSuchUser(pub u32);

pub struct Profile {
    pub name: String,
    pub city: Option<String>,
    pub zip: Option<String>,
}

pub fn no_such_user(id: u32, built: &Cell<usize>) -> NoSuchUser {
    built.set(built.get() + 1);
    NoSuchUser(id)
}

pub fn find<'a>(table: &'a HashMap<u32, Profile>, id: u32, built: &Cell<usize>) -> Result<&'a Profile, NoSuchUser> {
    table.get(&id).ok_or_else(|| no_such_user(id, built))
}

pub fn zip_number(profile: &Profile) -> Result<Option<u32>, ParseIntError> {
    profile.zip.as_deref().map(str::parse).transpose()
}

pub fn shout_city(table: &HashMap<u32, Profile>, id: u32) -> Option<String> {
    let city = table.get(&id)?.city.as_ref()?;
    Some(city.to_uppercase())
}

fn main() {
    let table = HashMap::new();
    println!("{:?}", shout_city(&table, 1));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> HashMap<u32, Profile> {
        HashMap::from([
            (1, Profile { name: "Ira".into(), city: Some("Riga".into()), zip: Some("1050".into()) }),
            (2, Profile { name: "Oleg".into(), city: None, zip: None }),
            (3, Profile { name: "Ann".into(), city: Some("Oslo".into()), zip: Some("12a".into()) }),
        ])
    }

    #[test]
    fn find_builds_the_error_only_when_needed() {
        let table = table();
        let built = Cell::new(0);
        assert_eq!(find(&table, 1, &built).map(|p| p.name.as_str()), Ok("Ira"));
        assert_eq!(find(&table, 2, &built).map(|p| p.name.as_str()), Ok("Oleg"));
        assert_eq!(built.get(), 0, "пользователи нашлись, ошибку строить не было причины (ok_or_else, не ok_or)");
        assert_eq!(find(&table, 9, &built).err(), Some(NoSuchUser(9)));
        assert_eq!(built.get(), 1);
    }

    #[test]
    fn zip_distinguishes_absent_from_broken() {
        let table = table();
        assert_eq!(zip_number(&table[&1]), Ok(Some(1050)));
        assert_eq!(zip_number(&table[&2]), Ok(None), "индекса нет: это не ошибка");
        assert!(zip_number(&table[&3]).is_err(), "индекс 12a испорчен: это ошибка");
    }

    #[test]
    fn shout_city_stops_at_the_first_gap() {
        let table = table();
        assert_eq!(shout_city(&table, 1), Some("RIGA".to_string()));
        assert_eq!(shout_city(&table, 2), None, "города нет");
        assert_eq!(shout_city(&table, 7), None, "пользователя нет");
    }
}
