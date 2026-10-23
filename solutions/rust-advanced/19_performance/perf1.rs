use std::mem::size_of;
use std::num::NonZeroU32;

#[allow(clippy::ptr_arg)]
pub fn vec_footprint<T>(v: &Vec<T>) -> usize {
    size_of::<Vec<T>>() + v.capacity() * size_of::<T>()
}

#[allow(clippy::ptr_arg)]
pub fn string_footprint(s: &String) -> usize {
    size_of::<String>() + s.capacity()
}

pub fn boxed_slice_footprint<T>(b: &[T]) -> usize {
    size_of::<&[T]>() + size_of_val(b)
}

pub fn niche_sizes() -> [(&'static str, usize, usize); 5] {
    [
        ("u32", size_of::<u32>(), size_of::<Option<u32>>()),
        ("Box<u32>", size_of::<Box<u32>>(), size_of::<Option<Box<u32>>>()),
        ("&u8", size_of::<&u8>(), size_of::<Option<&u8>>()),
        ("NonZeroU32", size_of::<NonZeroU32>(), size_of::<Option<NonZeroU32>>()),
        ("char", size_of::<char>(), size_of::<Option<char>>()),
    ]
}

fn main() {
    for (name, plain, optional) in niche_sizes() {
        println!("{name}: {plain} байт, Option<{name}>: {optional} байт");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vec_counts_capacity_not_length() {
        let mut v: Vec<u64> = Vec::with_capacity(100);
        v.extend([1, 2, 3]);
        assert_eq!(vec_footprint(&v), 24 + 800, "в куче занято место под всю ёмкость (100 * 8), а не под три элемента");
        let empty: Vec<u32> = Vec::new();
        assert_eq!(vec_footprint(&empty), 24, "пустой вектор без выделения весит только заголовок");
    }

    #[test]
    fn string_counts_capacity() {
        let mut s = String::with_capacity(64);
        s.push_str("привет");
        assert_eq!(string_footprint(&s), 24 + 64);
    }

    #[test]
    fn boxed_slice_has_no_spare_capacity() {
        let b: Box<[u16]> = vec![1u16, 2, 3, 4].into_boxed_slice();
        assert_eq!(boxed_slice_footprint(&b), 16 + 8, "заголовок Box<[T]> это указатель и длина, данные 4 * 2 байта");
        let v: Vec<u16> = Vec::with_capacity(100);
        assert!(vec_footprint(&v) > boxed_slice_footprint(&v[..]), "вектор с запасом тяжелее среза той же длины");
    }

    #[test]
    fn niche_report_is_correct() {
        let report = niche_sizes();
        let find = |name: &str| report.iter().find(|(n, _, _)| *n == name).copied().unwrap_or_else(|| panic!("в отчёте нет `{name}`"));
        let (_, plain, optional) = find("u32");
        assert_eq!((plain, optional), (4, 8), "у u32 нет свободных значений, Option добавляет метку и выравнивание");
        let (_, plain, optional) = find("Box<u32>");
        assert_eq!((plain, optional), (8, 8), "Box не бывает нулевым, поэтому None живёт в нуле: Option<Box<T>> не стоит ничего");
        let (_, plain, optional) = find("&u8");
        assert_eq!((plain, optional), (8, 8), "ссылка тоже не бывает нулевой");
        let (_, plain, optional) = find("NonZeroU32");
        assert_eq!((plain, optional), (4, 4), "NonZero освобождает ноль под None");
        let (_, plain, optional) = find("char");
        assert_eq!((plain, optional), (4, 4), "у char значения выше 0x10FFFF не бывает, туда помещается None");
    }
}
