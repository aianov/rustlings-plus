// Очередь событий разрослась: в ней миллионы значений типа `Event`, и память растёт быстрее, чем
// событий приходит. Большинство событий крошечные (тик, клавиша), но один редкий вариант тащит в себе
// целый снимок экрана, и размер enum равен размеру самого большого варианта. Каждое значение в очереди
// платит за снимок, даже когда его там нет. Сделай так, чтобы место зависело от того, что реально лежит.
pub const SNAPSHOT_LEN: usize = 512;

pub enum Event {
    Tick,
    Key(char),
    Message { from: u32, text: String },
    Snapshot([u8; SNAPSHOT_LEN]),
}

impl Event {
    pub fn snapshot(data: &[u8; SNAPSHOT_LEN]) -> Event {
        Event::Snapshot(*data)
    }
}

pub fn payload_bytes(events: &[Event]) -> usize {
    events
        .iter()
        .map(|event| match event {
            Event::Tick => 0,
            Event::Key(c) => c.len_utf8(),
            Event::Message { text, .. } => text.len(),
            Event::Snapshot(data) => data.len(),
        })
        .sum()
}

pub fn snapshot_checksum(events: &[Event]) -> u64 {
    events
        .iter()
        .filter_map(|event| match event {
            Event::Snapshot(data) => Some(data.iter().map(|&b| u64::from(b)).sum::<u64>()),
            _ => None,
        })
        .sum()
}

fn main() {
    let queue = [Event::Tick, Event::Key('x'), Event::snapshot(&[1; SNAPSHOT_LEN])];
    println!("{} байт на {} событий", std::mem::size_of_val(&queue[..]), queue.len());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    #[test]
    fn event_is_small() {
        assert!(
            size_of::<Event>() <= 32,
            "Event весит {} байт: самый большой вариант раздувает все события в очереди, вынеси его в Box",
            size_of::<Event>()
        );
    }

    #[test]
    fn million_events_stay_cheap() {
        let queue: Vec<Event> = (0..1_000).map(|n| if n % 100 == 0 { Event::snapshot(&[7; SNAPSHOT_LEN]) } else { Event::Tick }).collect();
        assert!(size_of_val(&queue[..]) <= 32 * 1_000, "на тысячу событий уходит {} байт", size_of_val(&queue[..]));
    }

    #[test]
    fn snapshot_content_survives() {
        let mut data = [0u8; SNAPSHOT_LEN];
        for (i, b) in data.iter_mut().enumerate() {
            *b = (i % 251) as u8;
        }
        let events = [Event::Tick, Event::snapshot(&data), Event::Key('я'), Event::Message { from: 1, text: "hi".to_string() }];
        let expected: u64 = data.iter().map(|&b| u64::from(b)).sum();
        assert_eq!(snapshot_checksum(&events), expected, "содержимое снимка должно остаться прежним");
        assert_eq!(payload_bytes(&events), SNAPSHOT_LEN + 4);
    }
}
