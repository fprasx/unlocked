#![feature(allocator_api)]
#![no_std]
#[allow(unused_imports)]
#[deny(unused_unsafe)]
#[macro_use]
pub mod secvec;

#[macro_export]
macro_rules! vector_impl {
    ($T:ty, $inner_ty:ty, $atomic_ty:ty) => {
        println!(
            "SecVec<{}> can be implemented with regular type {} and atomic type {}",
            stringify!($T),
            stringify!($inner_ty),
            stringify!($atomic_ty)
        );
    };
}

#[macro_export]
/// Takes a type and returns the correct SecVec implementation for it
macro_rules! get_impl_type {
    ($type:ty) => {
        use std::sync::atomic::*;
        let size = std::mem::size_of::<$type>();
        println!("{} has size {}", stringify!($type), size);
        if size == 0 {
        } else if size == 1 {
            lfvec::vector_impl!($type, u8, AtomicU8)
        } else if size == 2 {
            lfvec::vector_impl!($type, u16, AtomicU16)
        } else if size <= 4 {
            lfvec::vector_impl!($type, u32, AtomicU32)
        } else if size <= 8 {
            lfvec::vector_impl!($type, usize, AtomicUsize)
        } else {
            panic!(concat!(
                stringify!($type),
                " is too large, size cannot exceed 8 bytes"
            ));
        }
    };
}

/// Return the highest bit set in a number
/// ```
/// # use lfvec::highest_bit;
/// let x = 1 << 2;
/// assert_eq!(highest_bit(x), 2)
/// ```
pub fn highest_bit(num: usize) -> u32 {
    // Eliminate a jump/branch by not using if statement
    (num == 0) as u32 + 63 - num.leading_zeros()
}
