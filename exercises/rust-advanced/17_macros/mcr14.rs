// `#[derive(Debug, Clone, PartialEq)]` кажется магией, но это обычный макрос, который пишет код за тебя.
// Чтобы понять, какой, напиши его результат руками для структуры `Profile` и enum `Reaction`.
// Требования проверяет сравнение с настоящим derive: форматы `{:?}` и `{:#?}` должны совпасть до
// символа, копия не должна зависеть от оригинала, а равенство должно учитывать каждое поле и вариант.
// TODO: Реализуй `Debug`, `Clone` и `PartialEq` вручную для обоих типов (без derive).
use std::fmt;

pub struct Profile {
    pub id: u64,
    pub login: String,
    pub tags: Vec<String>,
}

pub enum Reaction {
    Like,
    Emoji(String),
    Custom { code: u32, label: String },
}

impl fmt::Debug for Profile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!()
    }
}

impl Clone for Profile {
    fn clone(&self) -> Profile {
        todo!()
    }
}

impl PartialEq for Profile {
    fn eq(&self, other: &Profile) -> bool {
        todo!()
    }
}

impl fmt::Debug for Reaction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!()
    }
}

impl Clone for Reaction {
    fn clone(&self) -> Reaction {
        todo!()
    }
}

impl PartialEq for Reaction {
    fn eq(&self, other: &Reaction) -> bool {
        todo!()
    }
}

fn main() {
    let profile = Profile { id: 1, login: "ann".into(), tags: vec![] };
    println!("{profile:?}");
}

#[cfg(test)]
mod tests {
    use super::*;

    mod derived {
        #[derive(Debug, Clone, PartialEq)]
        pub struct Profile {
            pub id: u64,
            pub login: String,
            pub tags: Vec<String>,
        }

        #[derive(Debug, Clone, PartialEq)]
        pub enum Reaction {
            Like,
            Emoji(String),
            Custom { code: u32, label: String },
        }
    }

    fn profile() -> Profile {
        Profile { id: 7, login: "ann".into(), tags: vec!["rust".into(), "cats".into()] }
    }

    fn derived_profile() -> derived::Profile {
        derived::Profile { id: 7, login: "ann".into(), tags: vec!["rust".into(), "cats".into()] }
    }

    fn reactions() -> (Vec<Reaction>, Vec<derived::Reaction>) {
        (
            vec![Reaction::Like, Reaction::Emoji("🔥".into()), Reaction::Custom { code: 9, label: "wow".into() }],
            vec![derived::Reaction::Like, derived::Reaction::Emoji("🔥".into()), derived::Reaction::Custom { code: 9, label: "wow".into() }],
        )
    }

    #[test]
    fn debug_matches_the_derived_output_exactly() {
        assert_eq!(format!("{:?}", profile()), format!("{:?}", derived_profile()));
        assert_eq!(format!("{:#?}", profile()), format!("{:#?}", derived_profile()), "красивый вывод тоже должен совпасть");
        let (mine, theirs) = reactions();
        for (a, b) in mine.iter().zip(&theirs) {
            assert_eq!(format!("{a:?}"), format!("{b:?}"));
            assert_eq!(format!("{a:#?}"), format!("{b:#?}"));
        }
    }

    #[test]
    fn clone_is_deep_and_equal() {
        let original = profile();
        let mut copy = original.clone();
        assert_eq!(copy, original);
        copy.tags.push("new".into());
        copy.login.push('!');
        assert_eq!(original.tags.len(), 2, "правка копии не должна задевать оригинал");
        assert_eq!(original.login, "ann");
        let (mine, _) = reactions();
        for reaction in &mine {
            assert_eq!(&reaction.clone(), reaction);
        }
    }

    #[test]
    fn equality_looks_at_every_field() {
        let base = profile();
        assert_ne!(base, Profile { id: 8, ..profile() });
        assert_ne!(base, Profile { login: "bob".into(), ..profile() });
        assert_ne!(base, Profile { tags: vec!["rust".into()], ..profile() });
        assert_eq!(base, profile());
    }

    #[test]
    fn equality_for_enums_distinguishes_variants_and_payloads() {
        let (variants, _) = reactions();
        for (i, a) in variants.iter().enumerate() {
            for (j, b) in variants.iter().enumerate() {
                assert_eq!(a == b, i == j, "{a:?} против {b:?}");
            }
        }
        assert_ne!(Reaction::Emoji("a".into()), Reaction::Emoji("b".into()));
        assert_ne!(Reaction::Custom { code: 1, label: "x".into() }, Reaction::Custom { code: 2, label: "x".into() });
        assert_ne!(Reaction::Custom { code: 1, label: "x".into() }, Reaction::Custom { code: 1, label: "y".into() });
    }
}
