// Игровое поле хранит клетки одним вектором по строкам, а снаружи хочется писать `grid[(x, y)]` и
// `grid[(x, y)] = value`, а ещё `grid[y]` для целой строки как среза. Если проверять только общий номер
// клетки в векторе, обращение `grid[(width, 0)]` тихо вернёт первую клетку следующей строки.
// TODO: Реализуй `Index` и `IndexMut` для пары `(usize, usize)` (колонка, строка) и `Index<usize>` для
// строки. Выход за границы по любой оси это паника с понятным сообщением, а не чужая клетка.
use std::ops::{Index, IndexMut};

pub struct Grid<T> {
    width: usize,
    height: usize,
    cells: Vec<T>,
}

impl<T: Clone> Grid<T> {
    pub fn new(width: usize, height: usize, fill: T) -> Self {
        Grid { width, height, cells: vec![fill; width * height] }
    }
}

fn main() {
    let grid = Grid::new(2, 2, 0u8);
    println!("{}", grid.width);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_and_write_cells() {
        let mut grid = Grid::new(3, 2, '.');
        grid[(2, 1)] = '#';
        grid[(0, 0)] = '@';
        assert_eq!(grid[(2, 1)], '#');
        assert_eq!(grid[(0, 0)], '@');
        assert_eq!(grid[(1, 1)], '.');
    }

    #[test]
    fn rows_are_slices() {
        let mut grid = Grid::new(3, 2, 0u8);
        grid[(1, 1)] = 5;
        assert_eq!(&grid[1], &[0, 5, 0]);
        assert_eq!(grid[0].len(), 3);
        assert_eq!(grid[0].iter().sum::<u8>(), 0);
    }

    #[test]
    fn compound_assignment_through_index_mut() {
        let mut grid = Grid::new(2, 2, 10);
        grid[(1, 0)] += 5;
        assert_eq!(grid[(1, 0)], 15);
    }

    #[test]
    #[should_panic(expected = "вне поля")]
    fn column_out_of_range_does_not_wrap_to_next_row() {
        let grid = Grid::new(3, 3, 0);
        let _ = grid[(3, 0)];
    }

    #[test]
    #[should_panic(expected = "вне поля")]
    fn row_out_of_range_panics() {
        let grid = Grid::new(3, 3, 0);
        let _ = grid[(0, 3)];
    }

    #[test]
    #[should_panic(expected = "вне поля")]
    fn whole_row_out_of_range_panics() {
        let grid = Grid::new(3, 3, 0);
        let _ = &grid[3];
    }
}
