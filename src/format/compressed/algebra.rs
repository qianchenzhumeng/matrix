use algebra::{MultiplyInto, MultiplySelf};
use format::{Compressed, Diagonal};
use {Element, Number};

impl<T> MultiplySelf<Diagonal<T>> for Compressed<T>
    where T: Element + Number
{
    fn multiply_self(&mut self, right: &Diagonal<T>) {
        let (m, n) = (self.rows, right.columns);
        debug_assert_eq!(self.columns, right.rows);
        self.resize((m, n));
        for (_, j, value) in self.iter_mut() {
            *value = *value * right[j];
        }
    }
}

impl<'l, T> MultiplyInto<[T], [T]> for Compressed<T> where T: Element + Number {
    fn multiply_into(&self, right: &[T], result: &mut [T]) {
        let (m, p) = (self.rows, self.columns);
        let n = right.len() / p;
        multiply_matrix(self, right, result, m, p, n)
    }
}

#[inline(always)]
fn multiply_matrix<T>(a: &Compressed<T>, b: &[T], c: &mut [T], m: usize, p: usize, n: usize)
    where T: Element + Number
{
    debug_assert_eq!(a.rows * a.columns, m * p);
    debug_assert_eq!(b.len(), p * n);
    debug_assert_eq!(c.len(), m * n);
    let (mut i, mut j) = (0, 0);
    for _ in 0..n {
        multiply_vector(a, &b[i..(i + p)], &mut c[j..(j + m)], p);
        i += p;
        j += m;
    }
}

#[inline(always)]
fn multiply_vector<T>(a: &Compressed<T>, b: &[T], c: &mut [T], p: usize)
    where T: Element + Number
{
    let &Compressed { ref values, ref indices, ref offsets, .. } = a;
    for j in 0..p {
        for k in offsets[j]..offsets[j + 1] {
            let current = c[indices[k]];
            c[indices[k]] = current + values[k] * b[j];
        }
    }
}

#[cfg(test)]
mod tests {
    use prelude::*;
    use format::compressed::Variant;

    #[test]
    fn multiply_self_diagonal() {
        let mut matrix = new!(3, 2, 3, Variant::Column, vec![1.0, 2.0, 3.0],
                              vec![1, 0, 2], vec![0, 1, 3]);

        let right = Diagonal::from_vec((2, 4), vec![4.0, 5.0]);

        matrix.multiply_self(&right);

        assert_eq!(matrix, new!(3, 4, 3, Variant::Column, vec![4.0, 10.0, 15.0],
                                vec![1, 0, 2], vec![0, 1, 3, 3, 3]));
    }

    #[test]
    fn multiply_into_conventional() {
        let matrix = Compressed::from(Conventional::from_vec((4, 3), vec![
            1.0, 2.0, 3.0, 4.0,
            5.0, 6.0, 6.0, 5.0,
            4.0, 3.0, 2.0, 1.0,
        ]));

        let right = Conventional::from_vec((3, 2), vec![
            1.0, 2.0, 3.0,
            4.0, 5.0, 6.0,
        ]);

        let mut result = Conventional::from_vec((4, 2), vec![
            1.0, 1.0, 1.0, 1.0,
            1.0, 1.0, 1.0, 1.0,
        ]);

        matrix.multiply_into(&right, &mut result);

        assert_eq!(&result.values, &vec![
            24.0, 24.0, 22.0, 18.0,
            54.0, 57.0, 55.0, 48.0,
        ]);
    }
}