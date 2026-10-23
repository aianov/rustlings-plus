use serde::{Deserialize, Deserializer};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Field<T> {
    #[default]
    Missing,
    Null,
    Value(T),
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Field<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Field<T>, D::Error> {
        Option::<T>::deserialize(deserializer).map(|value| match value {
            Some(value) => Field::Value(value),
            None => Field::Null,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfilePatch {
    #[serde(default)]
    pub display_name: Field<String>,
    #[serde(default)]
    pub bio: Field<String>,
}
