//! Module for hashing helpers

use std::{
    hash::{BuildHasherDefault, Hasher},
    marker::PhantomData,
    num::{
        NonZeroI8, NonZeroI16, NonZeroI32, NonZeroI64, NonZeroIsize, NonZeroU8, NonZeroU16,
        NonZeroU32, NonZeroU64, NonZeroUsize,
    },
};

mod sealed {
    pub trait SealedInteger {}
}

/// Int Hasher
#[derive(Debug, Default, Clone, Copy)]
pub struct IntHasher<T: IsInteger>(u64, PhantomData<T>);

/// IntHasher builder use for
/// - [`IntHashSet`]
/// - [`IntHashMap`]
pub type BuildIntHasher<T> = BuildHasherDefault<IntHasher<T>>;

/// IntHashSet for integer HashSet
///
/// ```
/// use prosa_utils::hash::{BuildIntHasher, IntHashSet};
///
/// let mut int_hashset: IntHashSet<u32> = IntHashSet::with_capacity_and_hasher(1, BuildIntHasher::default());
/// assert!(int_hashset.insert(2));
/// assert!(int_hashset.insert(3));
/// assert!(int_hashset.capacity() >= 2);
/// ```
pub type IntHashSet<T> = std::collections::HashSet<T, BuildIntHasher<T>>;

/// IntHashMap for integer HashMap
///
/// ```
/// use prosa_utils::hash::{BuildIntHasher, IntHashMap};
///
/// let mut int_hashmap: IntHashMap<i16, String> = IntHashMap::with_capacity_and_hasher(1, BuildIntHasher::default());
/// assert!(int_hashmap.insert(2, "test2".to_string()).is_none());
/// assert!(int_hashmap.insert(3, "test3".to_string()).is_none());
/// assert!(int_hashmap.capacity() >= 2);
/// ```
pub type IntHashMap<K, V> = std::collections::HashMap<K, V, BuildIntHasher<K>>;

/// Trait to identify integer types for implementations.
///
/// This trait is sealed and can only be implemented by `prosa_utils` for supported integer types.
///
/// ```
/// use prosa_utils::hash::IsInteger;
///
/// /// When you implement the trait, P need to be an integer
/// pub trait IntegerGetter<P: IsInteger> {
///     /// Returns an integer
///     fn get_int(&self) -> P;
/// }
/// ```
///
/// ```compile_fail
/// struct CustomInteger(u64);
///
/// impl prosa_utils::hash::IsInteger for CustomInteger {}
/// ```
pub trait IsInteger: sealed::SealedInteger {}

macro_rules! impl_is_integer {
    ( $($integer:ty),+ $(,)? ) => {
        $(
            impl sealed::SealedInteger for $integer {}
            impl IsInteger for $integer {}
        )+
    };
}

impl_is_integer!(
    u8,
    u16,
    u32,
    u64,
    usize,
    i8,
    i16,
    i32,
    i64,
    isize,
    NonZeroU8,
    NonZeroU16,
    NonZeroU32,
    NonZeroU64,
    NonZeroUsize,
    NonZeroI8,
    NonZeroI16,
    NonZeroI32,
    NonZeroI64,
    NonZeroIsize,
);

impl<T: IsInteger> Hasher for IntHasher<T> {
    fn write(&mut self, _: &[u8]) {
        panic!("Invalid use of IntHasher")
    }

    fn write_u8(&mut self, n: u8) {
        self.0 = u64::from(n)
    }
    fn write_u16(&mut self, n: u16) {
        self.0 = u64::from(n)
    }
    fn write_u32(&mut self, n: u32) {
        self.0 = u64::from(n)
    }
    fn write_u64(&mut self, n: u64) {
        self.0 = n
    }
    fn write_usize(&mut self, n: usize) {
        self.0 = n as u64
    }

    fn write_i8(&mut self, n: i8) {
        self.0 = n as u64
    }
    fn write_i16(&mut self, n: i16) {
        self.0 = n as u64
    }
    fn write_i32(&mut self, n: i32) {
        self.0 = n as u64
    }
    fn write_i64(&mut self, n: i64) {
        self.0 = n as u64
    }
    fn write_isize(&mut self, n: isize) {
        self.0 = n as u64
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hashset() {
        let mut int_hashset =
            IntHashSet::<u32>::with_capacity_and_hasher(1, BuildIntHasher::default());
        assert!(int_hashset.insert(2));
        assert!(int_hashset.insert(3));
        assert!(int_hashset.capacity() >= 2);
    }

    #[test]
    fn test_hashmap() {
        let mut int_hashmap =
            IntHashMap::<i16, String>::with_capacity_and_hasher(1, BuildIntHasher::default());
        assert!(int_hashmap.insert(2, "test2".to_string()).is_none());
        assert!(int_hashmap.insert(3, "test3".to_string()).is_none());
        assert!(int_hashmap.capacity() >= 2);
    }
}
