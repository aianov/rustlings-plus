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

impl<T> Grid<T> {
    fn offset(&self, x: usize, y: usize) -> usize {
        assert!(x < self.width && y < self.height, "клетка ({x}, {y}) вне поля {}x{}", self.width, self.height);
        y * self.width + x
    }
}

impl<T> Index<(usize, usize)> for Grid<T> {
    type Output = T;

    fn index(&self, (x, y): (usize, usize)) -> &T {
        &self.cells[self.offset(x, y)]
    }
}

impl<T> IndexMut<(usize, usize)> for Grid<T> {
    fn index_mut(&mut self, (x, y): (usize, usize)) -> &mut T {
        let at = self.offset(x, y);
        &mut self.cells[at]
    }
}

impl<T> Index<usize> for Grid<T> {
    type Output = [T];

    fn index(&self, y: usize) -> &[T] {
        assert!(y < self.height, "строка {y} вне поля высотой {}", self.height);
        &self.cells[y * self.width..(y + 1) * self.width]
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
