// Запись из внешней системы приходит одной строкой `ann;42;berlin`, а читать её хочется по полям, не делая
// копий каждого поля. Первая мысль: хранить `raw: String` и рядом `Vec<&str>` с кусками. Так нельзя:
// структура не может держать ссылку на собственное поле, потому что при перемещении структуры строка
// переедет, а ссылка останется на старом месте. Придумай, что хранить вместо ссылок.
// Поля разделены `;`, пробелы не убираются, `a;;b` даёт три поля, пустая строка даёт одно пустое поле.
// TODO: Добавь в `Record` нужные поля и реализуй `parse`, `len`, `is_empty`, `field`, `fields`, `raw`.
use std::ops::Range;

pub struct Record {
    raw: String,
}

impl Record {
    pub fn parse(raw: String) -> Record {
        todo!("запомнить диапазоны полей")
    }

    pub fn len(&self) -> usize {
        todo!()
    }

    pub fn is_empty(&self) -> bool {
        todo!()
    }

    pub fn field(&self, index: usize) -> Option<&str> {
        todo!()
    }

    pub fn fields(&self) -> impl Iterator<Item = &str> {
        std::iter::empty()
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
