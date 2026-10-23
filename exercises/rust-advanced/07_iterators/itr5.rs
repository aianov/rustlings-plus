// Плейлист хранит треки в векторе, а внутрь пускать не хочет: поле приватное. Снаружи хочется писать
// обычные `for`: `for track in &playlist` читает, `for track in &mut playlist` меняет на месте,
// `for track in playlist` забирает треки. Всё это даёт трейт `IntoIterator`, реализованный три раза: для
// самого плейлиста, для ссылки на него и для изменяемой ссылки. Ещё нужен привычный метод `iter()`.
// TODO: Реализуй `IntoIterator` для `Playlist`, `&Playlist` и `&mut Playlist` и метод `iter`.

pub struct Track {
    pub title: String,
    pub plays: u32,
}

pub struct Playlist {
    tracks: Vec<Track>,
}

impl Playlist {
    pub fn new() -> Playlist {
        Playlist { tracks: Vec::new() }
    }

    pub fn add(&mut self, title: &str) {
        self.tracks.push(Track { title: title.to_string(), plays: 0 });
    }
}

fn main() {
    let mut list = Playlist::new();
    list.add("intro");
    println!("{}", list.tracks.len());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Playlist {
        let mut p = Playlist::new();
        for t in ["one", "two", "three"] {
            p.add(t);
        }
        p
    }

    #[test]
    fn for_over_a_reference_reads() {
        let list = sample();
        let mut titles = Vec::new();
        for track in &list {
            titles.push(track.title.clone());
        }
        assert_eq!(titles, vec!["one", "two", "three"]);
        assert_eq!(list.iter().count(), 3, "плейлист остался на месте");
    }

    #[test]
    fn for_over_a_mutable_reference_changes_in_place() {
        let mut list = sample();
        for track in &mut list {
            track.plays += 7;
        }
        assert_eq!(list.iter().map(|t| t.plays).sum::<u32>(), 21);
    }

    #[test]
    fn for_by_value_takes_the_tracks() {
        let list = sample();
        let mut owned = Vec::new();
        for track in list {
            owned.push(track);
        }
        assert_eq!(owned.len(), 3);
        assert_eq!(owned[2].title, "three");
    }

    #[test]
    fn adapters_work_on_all_three_forms() {
        let mut list = sample();
        assert_eq!((&list).into_iter().filter(|t| t.title.len() == 3).count(), 2);
        (&mut list).into_iter().for_each(|t| t.plays = 1);
        let plays: Vec<u32> = list.into_iter().map(|t| t.plays).collect();
        assert_eq!(plays, vec![1, 1, 1]);
    }
}
