use std::ops::Range;

pub struct Record {
    raw: String,
    fields: Vec<Range<usize>>,
}

impl Record {
    pub fn parse(raw: String) -> Record {
        let mut fields = Vec::with_capacity(raw.matches(';').count() + 1);
        let mut start = 0;
        for (i, _) in raw.match_indices(';') {
            fields.push(start..i);
            start = i + 1;
        }
        fields.push(start..raw.len());
        Record { raw, fields }
    }

    pub fn len(&self) -> usize {
        self.fields.len()
    }

    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    pub fn field(&self, index: usize) -> Option<&str> {
        self.fields.get(index).map(|range| &self.raw[range.clone()])
    }

    pub fn fields(&self) -> impl Iterator<Item = &str> {
        self.fields.iter().map(|range| &self.raw[range.clone()])
    }

    pub fn raw(&self) -> &str {
        &self.raw
    }
}

fn main() {
    let r = Record::parse("ann;42".to_string());
    println!("{:?}", r.field(0));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inside(record: &Record, piece: &str) -> bool {
        let start = record.raw().as_ptr() as usize;
        let at = piece.as_ptr() as usize;
        at >= start && at + piece.len() <= start + record.raw().len()
    }

    #[test]
    fn splits_into_fields() {
        let r = Record::parse("ann;42;berlin".to_string());
        assert_eq!(r.len(), 3);
        assert_eq!(r.field(0), Some("ann"));
        assert_eq!(r.field(2), Some("berlin"));
        assert_eq!(r.field(3), None);
        assert_eq!(r.fields().collect::<Vec<_>>(), vec!["ann", "42", "berlin"]);
    }

    #[test]
    fn empty_fields_are_kept() {
        let r = Record::parse("a;;b;".to_string());
        assert_eq!(r.fields().collect::<Vec<_>>(), vec!["a", "", "b", ""], "пустые поля тоже поля");
        let empty = Record::parse(String::new());
        assert_eq!(empty.len(), 1);
        assert_eq!(empty.field(0), Some(""));
        assert!(!empty.is_empty());
    }

    #[test]
    fn fields_are_slices_of_the_original_string() {
        let r = Record::parse("ann;42;berlin".to_string());
        assert!(r.fields().all(|f| inside(&r, f)), "поля должны быть кусками исходной строки, а не копиями");
    }

    #[test]
    fn survives_being_moved_around() {
        let r = Record::parse("x;y".to_string());
        let mut all = vec![r];
        for i in 0..100 {
            all.push(Record::parse(format!("{i};{i}")));
        }
        let first = all.remove(0);
        assert_eq!(first.field(1), Some("y"), "после перемещения записи поля читаются по-прежнему");
        assert!(inside(&first, first.field(1).unwrap()));
        assert_eq!(all[10].field(0), Some("10"));
    }

    #[test]
    fn non_ascii_text() {
        let r = Record::parse("Аня;Тверь".to_string());
        assert_eq!(r.field(1), Some("Тверь"));
    }
}
