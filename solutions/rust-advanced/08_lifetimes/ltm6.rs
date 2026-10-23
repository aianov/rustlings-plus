pub struct Track {
    pub title: String,
    pub secs: u32,
}

pub struct Playlist {
    tracks: Vec<Track>,
}

pub struct TrackIter<'a> {
    rest: &'a [Track],
}

impl Playlist {
    pub fn new() -> Playlist {
        Playlist { tracks: Vec::new() }
    }

    pub fn push(&mut self, title: &str, secs: u32) {
        self.tracks.push(Track { title: title.to_string(), secs });
    }

    pub fn iter(&self) -> TrackIter<'_> {
        TrackIter { rest: &self.tracks }
    }
}

impl Default for Playlist {
    fn default() -> Playlist {
        Playlist::new()
    }
}

impl<'a> Iterator for TrackIter<'a> {
    type Item = &'a Track;

    fn next(&mut self) -> Option<&'a Track> {
        let (first, rest) = self.rest.split_first()?;
        self.rest = rest;
        Some(first)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.rest.len(), Some(self.rest.len()))
    }
}

impl DoubleEndedIterator for TrackIter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let (last, rest) = self.rest.split_last()?;
        self.rest = rest;
        Some(last)
    }
}

impl ExactSizeIterator for TrackIter<'_> {}

impl<'a> IntoIterator for &'a Playlist {
    type Item = &'a Track;
    type IntoIter = TrackIter<'a>;

    fn into_iter(self) -> TrackIter<'a> {
        self.iter()
    }
}

fn main() {
    let mut p = Playlist::new();
    p.push("intro", 30);
    for t in &p {
        println!("{}", t.title);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Playlist {
        let mut p = Playlist::new();
        p.push("intro", 30);
        p.push("song", 200);
        p.push("outro", 45);
        p
    }

    #[test]
    fn walks_forward_and_backward() {
        let p = sample();
        let titles: Vec<&str> = p.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, vec!["intro", "song", "outro"]);
        let back: Vec<u32> = p.iter().rev().map(|t| t.secs).collect();
        assert_eq!(back, vec![45, 200, 30], "rev должен идти с конца");
    }

    #[test]
    fn works_in_for_loops_over_a_reference() {
        let p = sample();
        let mut total = 0;
        for track in &p {
            total += track.secs;
        }
        assert_eq!(total, 275);
    }

    #[test]
    fn two_items_can_be_held_at_once() {
        let p = sample();
        let mut it = p.iter();
        let first = it.next().expect("первый трек");
        let second = it.next().expect("второй трек");
        assert_eq!((first.title.as_str(), second.title.as_str()), ("intro", "song"), "ссылки должны жить дольше вызова next");
    }

    #[test]
    fn both_ends_meet_in_the_middle() {
        let p = sample();
        let mut it = p.iter();
        assert_eq!(it.next().map(|t| t.secs), Some(30));
        assert_eq!(it.next_back().map(|t| t.secs), Some(45));
        assert_eq!(it.next().map(|t| t.secs), Some(200));
        assert!(it.next().is_none());
        assert!(it.next_back().is_none(), "середина отдана, больше ничего нет");
    }

    #[test]
    fn knows_its_length() {
        let p = sample();
        let mut it = p.iter();
        assert_eq!(it.len(), 3);
        it.next();
        assert_eq!(it.len(), 2, "длина должна уменьшаться по мере чтения");
        assert_eq!(it.size_hint(), (2, Some(2)));
        assert_eq!(Playlist::new().iter().len(), 0);
    }
}
