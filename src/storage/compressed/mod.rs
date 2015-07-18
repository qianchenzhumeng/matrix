//! The compressed storage.
//!
//! The storage is suitable for generic sparse matrices. Data are stored in one
//! of the following formats:
//!
//! * the [compressed-column][1] format or
//! * the [compressed-row][2] format.
//!
//! [1]: http://netlib.org/linalg/html_templates/node92.html
//! [2]: http://netlib.org/linalg/html_templates/node91.html

use std::{iter, mem};

use {Element, Matrix, Position, Size};

/// A compressed matrix.
#[derive(Clone, Debug, PartialEq)]
pub struct Compressed<T: Element> {
    /// The number of rows.
    pub rows: usize,
    /// The number of columns.
    pub columns: usize,
    /// The number of nonzero elements.
    pub nonzeros: usize,
    /// The storage format.
    pub format: Format,
    /// The values of the nonzero elements.
    pub values: Vec<T>,
    /// The indices of rows when `format = Column` or columns when `format =
    /// Row` of the nonzero elements.
    pub indices: Vec<usize>,
    /// The offsets of columns when `format = Column` or rows when `format =
    /// Row` such that the values and indices of the `i`th column when `format =
    /// Column` or the `i`th row when `format = Row` are stored starting from
    /// `values[j]` and `indices[j]`, respectively, where `j = offsets[i]`. The
    /// vector has one additional element, which is always equal to `nonzeros`.
    pub offsets: Vec<usize>,
}

macro_rules! new(
    ($rows:expr, $columns:expr, $nonzeros:expr, $format:expr,
     $values:expr, $indices:expr, $offsets:expr) => (
        Compressed { rows: $rows, columns: $columns, nonzeros: $nonzeros, format: $format,
                     values: $values, indices: $indices, offsets: $offsets }
    );
);

mod convert;

#[cfg(debug_assertions)]
impl<T: Element> ::storage::Validate for Compressed<T> {
    fn validate(&self) {
        assert_eq!(self.nonzeros, self.values.len());
        assert_eq!(self.nonzeros, self.indices.len());
        match self.format {
            Format::Column => assert_eq!(self.columns + 1, self.offsets.len()),
            Format::Row => assert_eq!(self.rows + 1, self.offsets.len()),
        }
    }
}

/// A format of a compressed matrix.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Format {
    /// The compressed-column format.
    Column,
    /// The compressed-row format.
    Row,
}

/// A sparse iterator.
pub struct Iterator<'l, T: 'l + Element> {
    matrix: &'l Compressed<T>,
    taken: usize,
    major: usize,
}

/// A sparse iterator allowing mutation.
pub struct IteratorMut<'l, T: 'l + Element> {
    matrix: &'l mut Compressed<T>,
    taken: usize,
    major: usize,
}

size!(Compressed);

impl<T: Element> Compressed<T> {
    /// Create a zero matrix.
    #[inline]
    pub fn new<S: Size>(size: S, format: Format) -> Self {
        Compressed::with_capacity(size, format, 0)
    }

    /// Create a zero matrix with a specific capacity.
    pub fn with_capacity<S: Size>(size: S, format: Format, capacity: usize) -> Self {
        let (rows, columns) = size.dimensions();
        let offset = match format {
            Format::Column => vec![0; columns + 1],
            Format::Row => vec![0; rows + 1],
        };
        new!(rows, columns, 0, format, Vec::with_capacity(capacity),
             Vec::with_capacity(capacity), offset)
    }

    /// Read an element.
    pub fn get<P: Position>(&self, position: P) -> T {
        let (mut i, mut j) = position.coordinates();
        debug_assert!(i < self.rows && j < self.columns);
        if let Format::Row = self.format {
            mem::swap(&mut i, &mut j);
        }
        for k in self.offsets[j]..self.offsets[j + 1] {
            if self.indices[k] == i {
                return self.values[k];
            }
            if self.indices[k] > i {
                break;
            }
        }
        T::zero()
    }

    /// Assign a value to an element.
    ///
    /// Note that the function treats zero values as any other.
    pub fn set<P: Position>(&mut self, position: P, value: T) {
        let (mut i, mut j) = position.coordinates();
        debug_assert!(i < self.rows && j < self.columns);
        if let Format::Row = self.format {
            mem::swap(&mut i, &mut j);
        }
        let mut k = self.offsets[j];
        while k < self.offsets[j + 1] {
            if self.indices[k] == i {
                self.values[k] = value;
                return;
            }
            if self.indices[k] > i {
                break;
            }
            k += 1;
        }
        self.nonzeros += 1;
        self.values.insert(k, value);
        self.indices.insert(k, i);
        for offset in &mut self.offsets[(j + 1)..] {
            *offset += 1;
        }
    }

    /// Return a sparse iterator.
    #[inline]
    pub fn iter<'l>(&'l self) -> Iterator<'l, T> {
        Iterator { matrix: self, taken: 0, major: 0 }
    }

    /// Return a sparse iterator allowing mutation.
    #[inline]
    pub fn iter_mut<'l>(&'l mut self) -> IteratorMut<'l, T> {
        IteratorMut { matrix: self, taken: 0, major: 0 }
    }

    /// Resize the matrix.
    pub fn resize<S: Size>(&mut self, size: S) {
        let (rows, columns) = size.dimensions();
        if rows < self.rows || columns < self.columns {
            self.retain(|i, j, _| i < rows && j < columns);
        }
        let (from, into) = match self.format {
            Format::Column => (self.columns, columns),
            Format::Row => (self.rows, rows),
        };
        if from > into {
            self.offsets.truncate(into + 1);
        } else if from < into {
            self.offsets.extend(vec![self.nonzeros; into - from]);
        }
        self.columns = columns;
        self.rows = rows;
    }

    /// Retain the elements that satisfy a condition and discard the rest.
    pub fn retain<F>(&mut self, mut condition: F) where F: FnMut(usize, usize, &T) -> bool {
        let (mut k, mut major) = (0, 0);
        while k < self.indices.len() {
            while self.offsets[major + 1] <= k {
                major += 1;
            }
            let condition = match self.format {
                Format::Column => condition(self.indices[k], major, &self.values[k]),
                Format::Row => condition(major, self.indices[k], &self.values[k]),
            };
            if condition {
                k += 1;
                continue;
            }
            self.nonzeros -= 1;
            self.values.remove(k);
            self.indices.remove(k);
            for offset in &mut self.offsets[(major + 1)..] {
                *offset -= 1;
            }
        }
    }
}

impl<T: Element> Matrix for Compressed<T> {
    type Element = T;

    fn nonzeros(&self) -> usize {
        self.values.iter().fold(0, |sum, &value| if value.is_zero() { sum } else { sum + 1 })
    }

    fn transpose(&self) -> Self {
        let &Compressed { rows, columns, nonzeros, format, .. } = self;
        let mut matrix = Compressed::with_capacity((columns, rows), format, nonzeros);
        for (i, j, &value) in self.iter() {
            matrix.set((j, i), value);
        }
        matrix
    }

    #[inline]
    fn zero<S: Size>(size: S) -> Self {
        Compressed::new(size, Format::Column)
    }
}

impl Format {
    /// Return the other format.
    #[inline]
    pub fn flip(&self) -> Self {
        match *self {
            Format::Column => Format::Row,
            Format::Row => Format::Column,
        }
    }
}

macro_rules! iterator(
    (struct $name:ident -> $item:ty) => (
        impl<'l, T: Element> iter::Iterator for $name<'l, T> {
            type Item = $item;

            #[allow(mutable_transmutes)]
            fn next(&mut self) -> Option<Self::Item> {
                let &mut $name { ref matrix, ref mut taken, ref mut major } = self;
                let k = *taken;
                if k == matrix.nonzeros {
                    return None;
                }
                *taken += 1;
                while matrix.offsets[*major + 1] <= k {
                    *major += 1;
                }
                let item = unsafe { mem::transmute(&matrix.values[k]) };
                Some(match matrix.format {
                    Format::Column => (matrix.indices[k], *major, item),
                    Format::Row => (*major, matrix.indices[k], item),
                })
            }
        }
    );
);

iterator!(struct Iterator -> (usize, usize, &'l T));
iterator!(struct IteratorMut -> (usize, usize, &'l mut T));

#[cfg(test)]
mod tests {
    use prelude::*;
    use storage::compressed::Format;

    #[test]
    fn get() {
        let conventional = Conventional::from_vec(vec![
            0.0, 1.0, 0.0, 0.0, 0.0,
            0.0, 0.0, 0.0, 2.0, 3.0,
            0.0, 0.0, 0.0, 0.0, 4.0,
        ], (5, 3));

        let matrix: Compressed<_> = (&conventional).into();
        assert_eq!(matrix.nonzeros, 4);

        for i in 0..5 {
            for j in 0..3 {
                assert_eq!(conventional[(i, j)], matrix.get((i, j)));
            }
        }
    }

    #[test]
    fn set() {
        let mut conventional = Conventional::from_vec(vec![
            0.0, 1.0, 0.0, 0.0, 0.0,
            0.0, 0.0, 0.0, 2.0, 3.0,
            0.0, 0.0, 0.0, 0.0, 4.0,
        ], (5, 3));

        let mut matrix: Compressed<_> = (&conventional).into();
        assert_eq!(matrix.nonzeros, 4);

        conventional[(0, 0)] = 42.0;
        conventional[(3, 1)] = 69.0;

        matrix.set((0, 0), 42.0);
        matrix.set((3, 1), 69.0);
        matrix.set((4, 0), 0.0);

        assert_eq!(matrix.nonzeros, 4 + 1 + (1 - 1) + 1);
        assert_eq!(conventional, (&matrix).into());

        for i in 0..5 {
            for j in 0..3 {
                conventional[(i, j)] = (j * 5 + i) as f64;
                matrix.set((i, j), (j * 5 + i) as f64);
            }
        }

        assert_eq!(matrix.nonzeros, 5 * 3);
        assert_eq!(conventional, (&matrix).into());
    }

    #[test]
    fn nonzeros() {
        let matrix = new!(5, 7, 5, Format::Column, vec![1.0, 0.0, 3.0, 0.0, 5.0],
                          vec![1, 0, 3, 1, 4], vec![0, 0, 0, 1, 2, 2, 3, 5]);

        assert_eq!(matrix.nonzeros, 5);
        assert_eq!(matrix.nonzeros(), 3);
    }

    #[test]
    fn transpose() {
        let matrix = new!(5, 7, 5, Format::Column, vec![1.0, 2.0, 3.0, 4.0, 5.0],
                          vec![1, 0, 3, 1, 4], vec![0, 0, 0, 1, 2, 2, 3, 5]);

        let matrix = matrix.transpose();

        assert_eq!(matrix, new!(7, 5, 5, Format::Column, vec![2.0, 1.0, 4.0, 3.0, 5.0],
                                vec![3, 2, 6, 5, 6], vec![0, 1, 3, 3, 4, 5]));
    }

    #[test]
    fn iter() {
        let matrix = new!(5, 7, 5, Format::Column, vec![1.0, 2.0, 3.0, 4.0, 5.0],
                          vec![1, 0, 3, 1, 4], vec![0, 0, 0, 1, 2, 2, 3, 5]);

        let result = matrix.iter().map(|(i, _, _)| i).collect::<Vec<_>>();
        assert_eq!(&result, &vec![1, 0, 3, 1, 4]);

        let result = matrix.iter().map(|(_, j, _)| j).collect::<Vec<_>>();
        assert_eq!(&result, &vec![2, 3, 5, 6, 6]);

        let result = matrix.iter().map(|(_, _, &value)| value).collect::<Vec<_>>();
        assert_eq!(&result, &vec![1.0, 2.0, 3.0, 4.0, 5.0]);
    }

    #[test]
    fn iter_mut() {
        let mut matrix = new!(5, 7, 5, Format::Column, vec![1.0, 2.0, 3.0, 4.0, 5.0],
                              vec![1, 0, 3, 1, 4], vec![0, 0, 0, 1, 2, 2, 3, 5]);

        for (i, _, value) in matrix.iter_mut() {
            *value = if i % 2 == 0 { 42.0 } else { 69.0 };
        }

        assert_eq!(&matrix.values, &vec![69.0, 42.0, 69.0, 69.0, 42.0]);
    }

    #[test]
    fn resize_fewer_columns() {
        let mut matrix = new!(5, 7, 5, Format::Column, vec![1.0, 2.0, 3.0, 4.0, 5.0],
                              vec![1, 0, 3, 1, 4], vec![0, 0, 0, 1, 2, 2, 3, 5]);

        matrix.resize((5, 5));
        assert_eq!(matrix, new!(5, 5, 2, Format::Column, vec![1.0, 2.0],
                                vec![1, 0], vec![0, 0, 0, 1, 2, 2]));

        matrix.resize((5, 3));
        assert_eq!(matrix, new!(5, 3, 1, Format::Column, vec![1.0],
                                vec![1], vec![0, 0, 0, 1]));

        matrix.resize((5, 1));
        assert_eq!(matrix, new!(5, 1, 0, Format::Column, vec![],
                                vec![], vec![0, 0]));
    }

    #[test]
    fn resize_fewer_rows() {
        let mut matrix = new!(5, 7, 5, Format::Column, vec![1.0, 2.0, 3.0, 4.0, 5.0],
                              vec![1, 0, 3, 1, 4], vec![0, 0, 0, 1, 2, 2, 3, 5]);

        matrix.resize((3, 7));
        assert_eq!(matrix, new!(3, 7, 3, Format::Column, vec![1.0, 2.0, 4.0],
                                vec![1, 0, 1], vec![0, 0, 0, 1, 2, 2, 2, 3]));

        matrix.resize((1, 7));
        assert_eq!(matrix, new!(1, 7, 1, Format::Column, vec![2.0],
                                vec![0], vec![0, 0, 0, 0, 1, 1, 1, 1]));
    }

    #[test]
    fn resize_more_columns() {
        let mut matrix = new!(5, 7, 4, Format::Column, vec![1.0, 2.0, 3.0, 4.0],
                              vec![1, 1, 3, 4], vec![0, 0, 0, 1, 2, 2, 3, 4]);

        matrix.resize((5, 9));
        assert_eq!(matrix, new!(5, 9, 4, Format::Column, vec![1.0, 2.0, 3.0, 4.0],
                                vec![1, 1, 3, 4], vec![0, 0, 0, 1, 2, 2, 3, 4, 4, 4]));

        matrix.resize((5, 11));
        assert_eq!(matrix, new!(5, 11, 4, Format::Column, vec![1.0, 2.0, 3.0, 4.0],
                                vec![1, 1, 3, 4], vec![0, 0, 0, 1, 2, 2, 3, 4, 4, 4, 4, 4]));
    }

    #[test]
    fn resize_more_rows() {
        let mut matrix = new!(5, 7, 4, Format::Column, vec![1.0, 2.0, 3.0, 4.0],
                              vec![1, 1, 3, 4], vec![0, 0, 0, 1, 2, 2, 3, 4]);

        matrix.resize((7, 7));
        assert_eq!(matrix, new!(7, 7, 4, Format::Column, vec![1.0, 2.0, 3.0, 4.0],
                                vec![1, 1, 3, 4], vec![0, 0, 0, 1, 2, 2, 3, 4]));

        matrix.resize((9, 7));
        assert_eq!(matrix, new!(9, 7, 4, Format::Column, vec![1.0, 2.0, 3.0, 4.0],
                                vec![1, 1, 3, 4], vec![0, 0, 0, 1, 2, 2, 3, 4]));
    }
}