//! A fixed-size bitset over the games of the library: one bit per game, 64 games per word. 5,000 games are 79
//! words, so AND, OR and a popcount over one facet value take well under a microsecond.

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Bitset {
    words: Vec<u64>,
    len: usize,
}

impl Bitset {
    /// `len` bits, all clear.
    pub fn new(len: usize) -> Bitset {
        Bitset { words: vec![0; len.div_ceil(64)], len }
    }

    /// `len` bits, all set.
    pub fn full(len: usize) -> Bitset {
        let mut b = Bitset { words: vec![!0; len.div_ceil(64)], len };
        b.trim();
        b
    }

    /// Clears the bits past `len` in the last word, so popcount and iteration never see them.
    fn trim(&mut self) {
        let extra = self.words.len() * 64 - self.len;
        if extra > 0 {
            if let Some(last) = self.words.last_mut() {
                *last &= !0u64 >> extra;
            }
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.words.iter().all(|w| *w == 0)
    }

    pub fn set(&mut self, i: usize) {
        if i < self.len {
            self.words[i / 64] |= 1 << (i % 64);
        }
    }

    pub fn get(&self, i: usize) -> bool {
        i < self.len && self.words[i / 64] & (1 << (i % 64)) != 0
    }

    pub fn count(&self) -> u32 {
        self.words.iter().map(|w| w.count_ones()).sum()
    }

    pub fn and_with(&mut self, other: &Bitset) {
        for (a, b) in self.words.iter_mut().zip(&other.words) {
            *a &= b;
        }
    }

    pub fn or_with(&mut self, other: &Bitset) {
        for (a, b) in self.words.iter_mut().zip(&other.words) {
            *a |= b;
        }
    }

    /// Clears the bits set in `other`.
    pub fn and_not(&mut self, other: &Bitset) {
        for (a, b) in self.words.iter_mut().zip(&other.words) {
            *a &= !b;
        }
    }

    /// popcount(self AND other) without making the intersection.
    pub fn and_count(&self, other: &Bitset) -> u32 {
        self.words.iter().zip(&other.words).map(|(a, b)| (a & b).count_ones()).sum()
    }

    /// The set bits, lowest first.
    pub fn ones(&self) -> impl Iterator<Item = usize> + '_ {
        self.words.iter().enumerate().flat_map(|(wi, &w)| {
            let mut w = w;
            std::iter::from_fn(move || {
                if w == 0 {
                    return None;
                }
                let b = w.trailing_zeros() as usize;
                w &= w - 1;
                Some(wi * 64 + b)
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_count_and_iterate() {
        let mut b = Bitset::new(130);
        for i in [0, 63, 64, 129, 500] {
            b.set(i);
        }
        assert_eq!(b.count(), 4, "500 is past the end and ignored");
        assert!(b.get(63) && b.get(64) && !b.get(65) && !b.get(500));
        assert_eq!(b.ones().collect::<Vec<_>>(), [0, 63, 64, 129]);
        let full = Bitset::full(130);
        assert_eq!(full.count(), 130, "no bits past the end");
        assert_eq!(full.and_count(&b), 4);
        let mut x = full.clone();
        x.and_with(&b);
        assert_eq!(x, b);
        let mut y = Bitset::new(130);
        assert!(y.is_empty());
        y.or_with(&b);
        assert_eq!(y.count(), 4);
        assert_eq!(Bitset::full(128).count(), 128, "an exact number of words");
        let mut z = Bitset::full(130);
        z.and_not(&b);
        assert_eq!(z.count(), 126);
    }
}
