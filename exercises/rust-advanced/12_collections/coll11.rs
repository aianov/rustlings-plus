// Тренды: из потока постов с лайками нужны только `n` самых лайкаемых, а потоку нет конца, хранить его весь
// нельзя. Кто-то уже написал «ограниченную кучу»: кладём, а если элементов стало больше `n`, лишний
// выкидываем. Тест показывает, что выкидывается самое лучшее. При равенстве лайков выше пост с меньшим id.
// TODO: Почини `TopN`. В куче должно оставаться не больше `n` элементов, выбывает худший.
use std::cmp::Reverse;
use std::collections::BinaryHeap;

pub struct TopN {
    n: usize,
    heap: BinaryHeap<(u32, Reverse<u64>)>,
}

impl TopN {
    pub fn new(n: usize) -> TopN {
        TopN { n, heap: BinaryHeap::new() }
    }

    pub fn kept(&self) -> usize {
        self.heap.len()
    }

    pub fn push(&mut self, post: u64, likes: u32) {
        self.heap.push((likes, Reverse(post)));
        if self.heap.len() > self.n {
            self.heap.pop();
        }
    }

    pub fn ranked(self) -> Vec<(u64, u32)> {
        let mut v: Vec<(u64, u32)> = self.heap.into_iter().map(|(likes, Reverse(post))| (post, likes)).collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        v
    }
}

fn main() {
    let mut top = TopN::new(3);
    top.push(1, 10);
    println!("{:?}", top.ranked());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_best_n() {
        let mut top = TopN::new(3);
        for (post, likes) in [(1, 5), (2, 50), (3, 7), (4, 100), (5, 1), (6, 60)] {
            top.push(post, likes);
        }
        assert_eq!(top.ranked(), vec![(4, 100), (6, 60), (2, 50)], "в трендах должны остаться самые лайкаемые");
    }

    #[test]
    fn memory_stays_bounded() {
        let mut top = TopN::new(5);
        for i in 0..10_000_u64 {
            top.push(i, (i % 977) as u32);
            assert!(top.kept() <= 5, "в куче больше n элементов: поток нельзя копить целиком");
        }
        assert_eq!(top.kept(), 5);
    }

    #[test]
    fn ties_prefer_the_smaller_post_id() {
        let mut top = TopN::new(2);
        for post in [30, 10, 20, 40] {
            top.push(post, 7);
        }
        assert_eq!(top.ranked(), vec![(10, 7), (20, 7)], "при равных лайках в тренды попадают посты с меньшим id");
    }

    #[test]
    fn small_streams_and_zero() {
        let mut top = TopN::new(10);
        top.push(1, 3);
        top.push(2, 9);
        assert_eq!(top.ranked(), vec![(2, 9), (1, 3)]);
        let mut none = TopN::new(0);
        none.push(1, 1);
        assert_eq!(none.kept(), 0);
        assert!(none.ranked().is_empty());
    }
}
