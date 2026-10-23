pub struct Profile {
    pub nickname: Option<String>,
    pub bio: Option<String>,
    pub avatar: Option<Vec<u8>>,
}

impl Profile {
    pub fn display_name(&self) -> &str {
        match &self.nickname {
            Some(nick) => nick,
            None => "anonymous",
        }
    }

    pub fn bio_words(&self) -> usize {
        match &self.bio {
            Some(bio) => bio.split_whitespace().count(),
            None => 0,
        }
    }

    pub fn clip_bio(&mut self, max_bytes: usize) {
        if let Some(bio) = &mut self.bio {
            let mut cut = max_bytes.min(bio.len());
            while !bio.is_char_boundary(cut) {
                cut -= 1;
            }
            bio.truncate(cut);
        }
    }

    pub fn set_nickname(&mut self, new: String) -> Option<String> {
        self.nickname.replace(new)
    }
}

fn main() {
    let p = Profile { nickname: None, bio: None, avatar: None };
    println!("{}", p.display_name());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(nick: Option<&str>, bio: Option<&str>) -> Profile {
        Profile { nickname: nick.map(String::from), bio: bio.map(String::from), avatar: Some(vec![1, 2, 3]) }
    }

    #[test]
    fn display_name_borrows_the_nickname() {
        let p = profile(Some("ann"), None);
        let name = p.display_name();
        assert_eq!(name, "ann");
        assert_eq!(
            name.as_ptr(),
            p.nickname.as_ref().expect("ник задан").as_ptr(),
            "имя должно быть ссылкой на строку внутри профиля, а не копией"
        );
        assert_eq!(profile(None, None).display_name(), "anonymous");
    }

    #[test]
    fn profile_survives_reading() {
        let p = profile(Some("ann"), Some("loves rust"));
        let _ = p.display_name();
        let _ = p.bio_words();
        assert_eq!(p.nickname.as_deref(), Some("ann"), "чтение не должно забирать поля");
        assert_eq!(p.bio.as_deref(), Some("loves rust"));
    }

    #[test]
    fn counting_words() {
        assert_eq!(profile(None, Some("one two  three\nfour")).bio_words(), 4);
        assert_eq!(profile(None, Some("")).bio_words(), 0);
        assert_eq!(profile(None, None).bio_words(), 0, "без описания слов нет");
    }

    #[test]
    fn clipping_happens_in_place() {
        let mut p = profile(None, Some("hello world"));
        let before = p.bio.as_ref().expect("описание есть").as_ptr();
        p.clip_bio(5);
        let bio = p.bio.as_ref().expect("описание должно остаться");
        assert_eq!(bio, "hello");
        assert_eq!(bio.as_ptr(), before, "правка на месте: строка не должна перевыделяться и заменяться новой");
    }

    #[test]
    fn clipping_respects_char_boundaries() {
        let mut p = profile(None, Some("привет"));
        p.clip_bio(5);
        assert_eq!(p.bio.as_deref(), Some("пр"), "буква занимает 2 байта: пятый байт середина символа");
        p.clip_bio(100);
        assert_eq!(p.bio.as_deref(), Some("пр"), "длинный лимит ничего не меняет");
        let mut empty = profile(None, None);
        empty.clip_bio(3);
        assert!(empty.bio.is_none(), "нет описания значит и после обрезки нет");
    }

    #[test]
    fn swapping_nicknames() {
        let mut p = profile(Some("ann"), None);
        assert_eq!(p.set_nickname("bob".into()), Some("ann".to_string()), "возвращается прежний ник");
        assert_eq!(p.nickname.as_deref(), Some("bob"));
        let mut fresh = profile(None, None);
        assert_eq!(fresh.set_nickname("cat".into()), None);
        assert_eq!(fresh.nickname.as_deref(), Some("cat"));
        assert_eq!(fresh.avatar.as_deref(), Some(&[1, 2, 3][..]), "остальные поля не трогаем");
    }
}
