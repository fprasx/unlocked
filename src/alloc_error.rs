extern crate alloc;
use core::mem;

// Allocation error handling
// https://github.com/rust-lang/rust/blob/0ca7f74dbd23a3e8ec491cd3438f490a3ac22741/src/liballoc/raw_vec.rs#L502-L525
// We need to guarantee the following:
// * We don't ever allocate `> isize::MAX` byte-size objects.
// * We don't overflow `usize::MAX` and actually allocate too little.
//
// On 64-bit we just need to check for overflow since trying to allocate
// `> isize::MAX` bytes will surely fail. On 32-bit and 16-bit we need to add
// an extra guard for this in case we're running on a platform which can use
// all 4GB in user-space, e.g., PAE or x32.

#[inline]
pub(crate) fn alloc_guard(alloc_size: usize) -> Result<(), TryReserveError> {
    if mem::size_of::<usize>() < 8 && alloc_size > isize::MAX as usize {
        Err(TryReserveError {
            kind: TryReserveErrorKind::CapacityOverflow,
        })
    } else {
        Ok(())
    }
}

// One central function responsible for reporting capacity overflows. This'll
// ensure that the code generation related to these panics is minimal as there's
// only one location which panics rather than a bunch throughout the module.
pub(crate) fn capacity_overflow() -> ! {
    panic!("Capacity overflowed")
}

// https://doc.rust-lang.org/src/alloc/collections/mod.rs.html#58-147

/// The error type for `try_reserve` methods.
#[derive(PartialEq, Eq)]
pub(crate) struct TryReserveError {
    kind: TryReserveErrorKind,
}

/// Details of the allocation that caused a `TryReserveError`
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TryReserveErrorKind {
    /// Error due to the computed capacity exceeding the collection's maximum
    /// (usually `isize::MAX` bytes).
    CapacityOverflow,
}
