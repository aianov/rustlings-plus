// Экран настроек отправляет на сервер частичное обновление: «поменяй только громкость». В proto3 у обычного поля нет понятия
// «не задано»: ноль, false и пустая строка неотличимы от отсутствия и вообще не попадают в провод. Поэтому пользователь
// может включить рассылку, но выключить её уже нет, а громкость 0 не ставится никак. Тесты ниже показывают три такие дыры.
// TODO: Почини SettingsUpdate и apply так, чтобы «не прислали» и «прислали ноль» различались, не меняя номеров полей: старые
// клиенты, которые шлют эти же байты, должны читаться как раньше. Имя показа сохраняет прежнее правило (пустое значит «не менять»).

#![allow(dead_code)]
use prost::Message;

#[derive(Clone, PartialEq, Message)]
struct SettingsUpdate {
    #[prost(string, tag = "1")]
    display_name: String,
    #[prost(uint32, tag = "2")]
    volume: u32,
    #[prost(bool, tag = "3")]
    newsletter: bool,
    #[prost(string, tag = "4")]
    status: String,
}

#[derive(Debug, Clone, PartialEq)]
struct Settings {
    display_name: String,
    volume: u32,
    newsletter: bool,
    status: String,
}

fn apply(settings: &mut Settings, update: &SettingsUpdate) {
    if !update.display_name.is_empty() {
        settings.display_name = update.display_name.clone();
    }
    if update.volume != 0 {
        settings.volume = update.volume;
    }
    if update.newsletter {
        settings.newsletter = true;
    }
    if !update.status.is_empty() {
        settings.status = update.status.clone();
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn stored() -> Settings {
        Settings { display_name: "marta".into(), volume: 70, newsletter: true, status: "на связи".into() }
    }

    fn roundtrip(update: &SettingsUpdate) -> SettingsUpdate {
        SettingsUpdate::decode(&update.encode_to_vec()[..]).unwrap()
    }

    fn update(volume: Option<u32>, newsletter: Option<bool>, status: Option<&str>) -> SettingsUpdate {
        SettingsUpdate { display_name: String::new(), volume, newsletter, status: status.map(str::to_string) }
    }

    #[test]
    fn volume_zero_can_be_set() {
        let mut s = stored();
        apply(&mut s, &roundtrip(&update(Some(0), None, None)));
        assert_eq!(s.volume, 0, "громкость 0 («без звука») должна выставляться");
        assert!(s.newsletter, "остальные поля не заданы и остаются как были");
        assert_eq!(s.status, "на связи");
    }

    #[test]
    fn newsletter_can_be_switched_off() {
        let mut s = stored();
        apply(&mut s, &roundtrip(&update(None, Some(false), None)));
        assert!(!s.newsletter, "отписка от рассылки должна работать");
        assert_eq!(s.volume, 70);
    }

    #[test]
    fn status_can_be_cleared() {
        let mut s = stored();
        apply(&mut s, &roundtrip(&update(None, None, Some(""))));
        assert_eq!(s.status, "", "пустой статус при явной отправке очищает статус");
        let mut s2 = stored();
        apply(&mut s2, &roundtrip(&update(None, None, None)));
        assert_eq!(s2, stored(), "пустое обновление ничего не меняет");
    }

    #[test]
    fn explicit_defaults_are_written_to_the_wire() {
        assert_eq!(update(Some(0), None, None).encode_to_vec(), vec![0x10, 0x00], "заданный ноль занимает два байта: ключ и значение");
        assert_eq!(update(None, Some(false), None).encode_to_vec(), vec![0x18, 0x00]);
        assert_eq!(update(None, None, Some("")).encode_to_vec(), vec![0x22, 0x00]);
        assert!(update(None, None, None).encode_to_vec().is_empty(), "незаданное не пишется");
    }

    #[test]
    fn old_clients_are_still_understood() {
        let from_old_client = [0x0a, 0x01, b'm', 0x10, 0x32, 0x18, 0x01];
        let decoded = SettingsUpdate::decode(&from_old_client[..]).unwrap();
        assert_eq!(decoded.display_name, "m");
        assert_eq!(decoded.volume, Some(50));
        assert_eq!(decoded.newsletter, Some(true), "старый клиент прислал true, это теперь Some(true)");
        assert_eq!(decoded.status, None, "поля, которого нет в байтах, нет и в структуре");
    }
}
