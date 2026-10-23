pub struct Recent<T, const N: usize> {
    buf: [T; N],
    next: usize,
    len: usize,
}

impl<T: Copy + Default, const N: usize> Recent<T, N> {
    pub fn new() -> Self {
        Recent { buf: [T::default(); N], next: 0, len: 0 }
    }

    pub fn push(&mut self, value: T) -> Option<T> {
        if N == 0 {
            return Some(value);
        }
        let evicted = (self.len == N).then(|| self.buf[self.next]);
        self.buf[self.next] = value;
        self.next = (self.next + 1) % N;
        self.len = (self.len + 1).min(N);
        evicted
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn is_full(&self) -> bool {
        self.len == N
    }

    pub fn to_vec(&self) -> Vec<T> {
        let start = (self.next + N - self.len) % N.max(1);
        (0..self.len).map(|i| self.buf[(start + i) % N]).collect()
    }

    pub fn latest(&self) -> Option<&T> {
        (self.len > 0).then(|| &self.buf[(self.next + N - 1) % N])
    }
}

impl<T: Copy + Default, const N: usize> Default for Recent<T, N> {
    fn default() -> Self {
        Recent::new()
    }
}

pub fn take_array<T: Copy, const N: usize>(items: &[T]) -> Option<[T; N]> {
    items.get(..N)?.try_into().ok()
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
