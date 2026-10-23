// Лента «последние N событий» живёт в стеке: размер известен при компиляции, куча не нужна. Кольцевой
// буфер `Recent<T, N>` держит массив `[T; N]`; когда он полон, новая запись вытесняет самую старую
// и `push` возвращает вытесненное. Ёмкость `N = 0` допустима: буфер ничего не хранит, и любое значение
// сразу «вытесняется» (возвращается назад). Ещё одна функция превращает срез в массив нужной длины.
// TODO: Допиши нужные поля в `Recent` (индекс записи и счётчик придумай сам) и реализуй `Recent` (`new`, `push`, `len`, `is_full`, `to_vec` от старого к новому, `latest`) и `take_array`.

pub struct Recent<T, const N: usize> {
    items: [T; N],
}

impl<T: Copy + Default, const N: usize> Recent<T, N> {
    pub fn new() -> Self {
        todo!()
    }

    pub fn push(&mut self, value: T) -> Option<T> {
        todo!("вытесненное значение, если было")
    }

    pub fn len(&self) -> usize {
        todo!()
    }

    pub fn is_full(&self) -> bool {
        todo!()
    }

    pub fn to_vec(&self) -> Vec<T> {
        todo!("от самого старого к самому новому")
    }

    pub fn latest(&self) -> Option<&T> {
        todo!()
    }
}

pub fn take_array<T: Copy, const N: usize>(items: &[T]) -> Option<[T; N]> {
    todo!("первые N элементов, если их хватает")
}

fn main() {
    let mut recent: Recent<u32, 3> = Recent::new();
    recent.push(1);
    println!("{:?}", recent.to_vec());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_up_without_evicting() {
        let mut r: Recent<u32, 3> = Recent::new();
        assert_eq!(r.push(1), None);
        assert_eq!(r.push(2), None);
        assert_eq!(r.len(), 2);
        assert!(!r.is_full());
        assert_eq!(r.push(3), None);
        assert!(r.is_full());
        assert_eq!(r.to_vec(), vec![1, 2, 3]);
    }

    #[test]
    fn evicts_the_oldest_and_keeps_order() {
        let mut r: Recent<u32, 3> = Recent::new();
        for n in 1..=3 {
            r.push(n);
        }
        assert_eq!(r.push(4), Some(1), "вытесняется самое старое");
        assert_eq!(r.push(5), Some(2));
        assert_eq!(r.to_vec(), vec![3, 4, 5], "порядок от старого к новому после оборота");
        assert_eq!(r.latest(), Some(&5));
        for n in 6..=20 {
            r.push(n);
        }
        assert_eq!(r.to_vec(), vec![18, 19, 20]);
        assert_eq!(r.len(), 3);
    }

    #[test]
    fn empty_buffer_has_no_latest() {
        let r: Recent<u8, 4> = Recent::new();
        assert_eq!(r.latest(), None);
        assert!(r.to_vec().is_empty());
    }

    #[test]
    fn zero_capacity_returns_everything_back() {
        let mut r: Recent<u8, 0> = Recent::new();
        assert_eq!(r.push(9), Some(9), "ёмкость ноль: значение сразу возвращается");
        assert_eq!(r.len(), 0);
        assert!(r.to_vec().is_empty());
        assert_eq!(r.latest(), None);
    }

    #[test]
    fn different_capacities_are_different_types() {
        assert!(std::mem::size_of::<Recent<u8, 4>>() < std::mem::size_of::<Recent<u8, 64>>(), "массив живёт внутри значения");
    }

    #[test]
    fn take_array_from_slices() {
        assert_eq!(take_array::<u8, 2>(&[1, 2, 3]), Some([1, 2]));
        assert_eq!(take_array::<u8, 3>(&[1, 2]), None, "элементов не хватает");
        let head: Option<[i32; 0]> = take_array(&[1, 2]);
        assert_eq!(head, Some([]), "ноль элементов всегда есть");
    }
}
