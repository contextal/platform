//! Unsigned comparison and ordering
use std::ops::Range;

/// A trait for unsigned integer standard types
pub trait Unsigned: Ord + Copy {
    /// Equality check
    #[inline]
    fn ueq<O: Unsigned + TryInto<Self>>(self, rhs: O) -> bool {
        if let Ok(rhs) = rhs.try_into() {
            self == rhs
        } else {
            false
        }
    }

    /// Less than check
    #[inline]
    fn ult<O: Unsigned + TryInto<Self>>(self, rhs: O) -> bool {
        if let Ok(rhs) = rhs.try_into() {
            self < rhs
        } else {
            true
        }
    }

    /// Less or equal check
    #[inline]
    fn ule<O: Unsigned + TryInto<Self>>(self, rhs: O) -> bool {
        if let Ok(rhs) = rhs.try_into() {
            self <= rhs
        } else {
            true
        }
    }

    /// Greater than check
    #[inline]
    fn ugt<O: Unsigned + TryInto<Self>>(self, rhs: O) -> bool {
        if let Ok(rhs) = rhs.try_into() {
            self > rhs
        } else {
            false
        }
    }

    /// Greater or equal check
    #[inline]
    fn uge<O: Unsigned + TryInto<Self>>(self, rhs: O) -> bool {
        if let Ok(rhs) = rhs.try_into() {
            self >= rhs
        } else {
            false
        }
    }

    /// Return the minimum of two [`Unsigned`] values
    #[inline]
    fn umin<O: Unsigned + TryInto<Self>>(self, rhs: O) -> Self {
        self.min(rhs.try_into().unwrap_or(self))
    }
}

impl Unsigned for u8 {}
impl Unsigned for u16 {}
impl Unsigned for u32 {}
impl Unsigned for u64 {}
impl Unsigned for u128 {}
impl Unsigned for usize {}

/// Return the minimum of two [`Unsigned`] values
#[inline]
pub fn umin<T: Unsigned, O: Unsigned + TryInto<T>>(a: T, b: O) -> T {
    a.min(b.try_into().unwrap_or(a))
}

/// Intersection trait for [`PartialOrd`] [`Range`]s
pub trait RangeIntersection {
    /// Range overlap check
    fn overlaps_with(&self, other: &Self) -> bool;
    /// Range disjoint check
    fn disjoint_from(&self, other: &Self) -> bool;
    /// Range inclusion check
    fn contains_range(&self, other: &Self) -> bool;
}

impl<T: PartialOrd> RangeIntersection for Range<T> {
    #[inline]
    fn overlaps_with(&self, other: &Self) -> bool {
        self.contains(&other.start) || other.contains(&self.start)
    }

    #[inline]
    fn disjoint_from(&self, other: &Self) -> bool {
        !self.overlaps_with(other)
    }

    #[inline]
    fn contains_range(&self, other: &Self) -> bool {
        self.start <= other.start && self.end >= other.end
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const REF: Range<i32> = 10..20;
    const EMPTY: Range<i32> = 10..10;

    #[test]
    fn test_overlaps() {
        // Range overlap tests
        assert!(!REF.overlaps_with(&(0..5))); //   [----] [++++]
        assert!(!REF.overlaps_with(&(0..10))); //  [----][++++]
        assert!(REF.overlaps_with(&(5..15))); //   [--±±+++]
        assert!(REF.overlaps_with(&(10..15))); //  [±±++]
        assert!(REF.overlaps_with(&(12..17))); //  [+±±+]
        assert!(REF.overlaps_with(&(15..20))); //  [++±±]
        assert!(REF.overlaps_with(&(18..23))); //  [+++±][-]
        assert!(!REF.overlaps_with(&(20..25))); // [++++][----]
        assert!(!REF.overlaps_with(&(25..30))); // [++++] [----]
        assert!(REF.overlaps_with(&(5..25))); //   [--±±±±--]

        // Empty range tests
        assert!(!REF.overlaps_with(&(5..5))); //   [] [++++]
        assert!(REF.overlaps_with(&(10..10))); //  [][++++]
        assert!(REF.overlaps_with(&(15..15))); //  [++[]++]
        assert!(!REF.overlaps_with(&(20..20))); // [++++][]
        assert!(!REF.overlaps_with(&(25..25))); // [++++] []
    }

    #[test]
    fn test_contains() {
        // Range overlap tests
        assert!(!REF.contains_range(&(0..5))); //    [----] [++++]
        assert!(!REF.contains_range(&(0..10))); //   [----][++++]
        assert!(!REF.contains_range(&(5..15))); //   [--±±+++]
        assert!(REF.contains_range(&(10..15))); //   [±±++]
        assert!(REF.contains_range(&(12..17))); //   [+±±+]
        assert!(REF.contains_range(&(15..20))); //   [++±±]
        assert!(!REF.contains_range(&(18..23))); //  [+++±][-]
        assert!(!REF.contains_range(&(20..25))); //  [++++][----]
        assert!(!REF.contains_range(&(25..30))); //  [++++] [----]
        assert!(!REF.contains_range(&(5..25))); //   [--±±±±--]

        // Empty range tests
        assert!(!EMPTY.contains_range(&(5..5)));
        assert!(!EMPTY.contains_range(&(5..8)));
        assert!(!EMPTY.contains_range(&(5..10)));
        assert!(!EMPTY.contains_range(&(5..20)));
        assert!(!EMPTY.contains_range(&(10..20)));
        assert!(!EMPTY.contains_range(&(15..20)));
        assert!(EMPTY.contains_range(&(10..10)));
    }

    #[test]
    fn test_cmp() {
        assert!(3u8.ult(4u8));
        assert!(3u16.ule(3usize));
        assert!(3u8.ult(256u16));
        assert!(3u32.ule(4u8));
        assert!(3u64.ugt(1u8));
        assert!(3u32.uge(3usize));
        assert!(256u16.ugt(1u8));
        assert!(!(3u8.ugt(256u16)));
    }
}
