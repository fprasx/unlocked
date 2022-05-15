// Implementation based on work by Dechev et. al., 2006
// in their paper Lock-free Dynamically Resizable Arrays
// https://www.stroustrup.com/lock-free-vector.pdf
extern crate alloc;
use crate::alloc_error::{alloc_guard, capacity_overflow};
use crate::highest_bit;
use alloc::alloc::{handle_alloc_error, Allocator, Global, Layout};
use alloc::boxed::Box;
use core::fmt;
use core::marker::PhantomData;
use core::mem;
use core::ptr::{self, NonNull};
use core::sync::atomic::{AtomicPtr, AtomicU64, Ordering};
use crossbeam_utils::{Backoff, CachePadded};
use haphazard;

// Setting up hazard pointers
// This makes sure they all use the same Domain, guaranteeing the protection is valid.
#[non_exhaustive]
pub struct Family;
type Domain = haphazard::Domain<Family>;
type HazardPointer<'domain> = haphazard::HazardPointer<'domain, Family>;
type HazAtomicPtr<T> = haphazard::AtomicPtr<T, Family>;

/// The number of elements in the first allocation.
/// Must always be a power of 2.
pub const FIRST_BUCKET_SIZE: usize = 8;

#[allow(clippy::declare_interior_mutable_const)]
pub const ATOMIC_NULLPTR: AtomicPtr<AtomicU64> = AtomicPtr::new(ptr::null_mut::<AtomicU64>());

pub struct SecVec<'a, T: Sized + Copy + Send + Sync> {
    buffers: CachePadded<Box<[AtomicPtr<AtomicU64>; 60]>>,
    descriptor: CachePadded<HazAtomicPtr<Descriptor<'a, T>>>,
    pub domain: Domain,
    _marker: PhantomData<T>, // Data is stored as transmuted T's
}

#[derive(Debug)]
pub struct Descriptor<'a, T: Sized + Send> {
    pending: HazAtomicPtr<Option<WriteDescriptor<'a, T>>>,
    size: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct WriteDescriptor<'a, T: Sized> {
    new: u64,
    old: u64,
    location: &'a AtomicU64,
    _marker: PhantomData<T>, // New and old are transmuted T's

}

impl<'a, T> Descriptor<'a, T>
where
    T: Send,
{
    fn new(pending: *mut Option<WriteDescriptor<'a, T>>, size: usize) -> Self {
        Descriptor {
            // # Safety
            // This is safe because pending is always the result of calling WriteDescriptor::new_*_as_ptr
            // which used the pointer from Box::into_raw which is guaranteed to be valid.
            // Descriptors are only reclaimed through hazard pointer mechanisms or Box::from_raw
            // if they were never shared across threads
            pending: unsafe { HazAtomicPtr::new(pending) },
            size,
        }
    }

    fn new_as_ptr(pending: *mut Option<WriteDescriptor<'a, T>>, size: usize) -> *mut Self {
        Box::into_raw(Box::new(Descriptor::new(pending, size)))
    }
}

impl<'a, T> WriteDescriptor<'a, T> {
    fn new(new: u64, old: u64, location: &'a AtomicU64) -> Self {
        WriteDescriptor {
            new,
            old,
            location,
            _marker: PhantomData::<T>,
        }
    }

    fn new_none_as_ptr() -> *mut Option<Self> {
        Box::into_raw(Box::new(None))
    }

    fn new_some_as_ptr(new: u64, old: u64, location: &'a AtomicU64) -> *mut Option<Self> {
        Box::into_raw(Box::new(Some(WriteDescriptor::new(new, old, location))))
    }
}

impl<'a, T> fmt::Debug for SecVec<'a, T>
where
    T: Copy + Send + Sync,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SecVec")
            .field("buffers", &self.buffers)
            .field("descriptor", &self.descriptor)
            .field("PhantomData", &self._marker)
            .finish()
    }
}

impl<'a, T> SecVec<'a, T>
where
    T: Sized + Copy + Sync + Send,
{
    /// Return of new instance of a SecVec, with capacity 0 and size 0;
    pub fn new() -> Self {
        let pending = WriteDescriptor::<T>::new_none_as_ptr();
        let descriptor = Descriptor::<T>::new_as_ptr(pending, 0);
        let buffers = Box::new([ATOMIC_NULLPTR; 60]);
        let domain = Domain::new(&Family {});
        Self {
            // # Safety
            // Constructing HazAtomicPtr is safe because the *mut Descriptor came from Box::into_raw,
            // and this descriptor can only be reclaimed through hazptr mechanisms (retiring in the same domain)
            descriptor: CachePadded::new(unsafe { haphazard::AtomicPtr::new(descriptor) }),
            buffers: CachePadded::new(buffers),
            domain,
            _marker: PhantomData,
        }
    }

    /// Return a *const T to the index specified
    ///
    /// # Safety
    /// The index this is called on **must** be a valid index, meaning:
    /// there must already be a bucket allocated which would hold that index
    /// **and** the index must already have been initialized with push/set
    unsafe fn get(&self, i: usize) -> *const AtomicU64 {
        // Check for overflow
        let pos = i
            .checked_add(FIRST_BUCKET_SIZE)
            .expect("index too large, integer overflow");

        let hibit = highest_bit(pos);

        let offset = pos ^ (1 << hibit);

        // Select the correct buffer to index into
        // # Safety
        // Since hibit = highest_bit(pos), and pos >= FIRST_BUCKET_SIZE
        // The subtraction hibit - highest_bit(FIRST_BUCKET_SIZE) cannot underflow
        let buffer = &self.buffers[(hibit - highest_bit(FIRST_BUCKET_SIZE)) as usize];

        // Check that the offset doesn't exceed isize::MAX
        assert!(
            offset
                .checked_mul(mem::size_of::<T>())
                .map(|val| val < isize::MAX as usize)
                .is_some(),
            "pointer offset exceed isize::MAX bytes"
        );

        // Offset the pointer to return a pointer to the correct element
        unsafe {
            // # Safety
            // We know that we can offset the pointer because we will have allocated a bucket
            // to store the value. Since we only call values that are `self.descriptor.size` or smaller,
            // we know the offset will not go out of bounds because of the assert.
            buffer.load(Ordering::Acquire).add(offset)
        }
    }

    
    /// Complete the given write operation, set the current write operation to None
    fn complete_write(&self, pending: *mut Option<WriteDescriptor<T>>) {
        // If cas of actual value fails, someone else did the write
        // Result of cmpxchng doesn matter
        if let Some(writedesc) = unsafe { *pending } {
            let _ = AtomicU64::compare_exchange(
                writedesc.location,
                writedesc.old,
                writedesc.new,
                Ordering::AcqRel,
                Ordering::Relaxed,
            );

            let new_writedesc = WriteDescriptor::<T>::new_none_as_ptr();

            let mut hp = HazardPointer::new_in_domain(&self.domain);

            let old = unsafe {
                self.descriptor
                    .load(&mut hp)
                    .unwrap()
                    .pending
                    // # Safety
                    // new_writedesc conforms to the requirements of HazAtomicPtr::new()
                    // because it comes from Box::into_raw and is a valid WriteDescriptor
                    .swap_ptr(new_writedesc)
            };

            // # Safety
            // We are the only thread that will retire this pointer because
            // only one thread can get the result of the swap (this one).
            // Two threads couldn't have performed a swap and both got this pointer.
            unsafe { old.unwrap().retire_in(&self.domain) };
        }
    }

    pub fn push(&self, elem: T) {
        let backoff = Backoff::new(); // Backoff causes significant speedup
        loop {
            let mut dhp = HazardPointer::new_in_domain(&self.domain);
            let current_desc = unsafe { self.descriptor.load(&mut dhp) }
                .expect("invalid ptr for descriptor in push");

            // Use a block to make explicit that the use of the wdesc does not outlive the use of the desc.
            // This means that when the desc is dropped, there will be no references to the wdesc inside.
            // And we can deallocate the wdesc with `Box::from_raw`
            {
                let mut wdhp = HazardPointer::new_in_domain(&self.domain);
                let pending = unsafe { current_desc.pending.load(&mut wdhp) }
                    .expect("invalid ptr from write-desc in push");

                self.complete_write(pending as *const _ as *mut _);
                // Hazard pointer is dropped, protection ends
            }

            // If we need more memory, calculate the bucket
            let bucket = (highest_bit(current_desc.size + FIRST_BUCKET_SIZE)
                - highest_bit(FIRST_BUCKET_SIZE)) as usize;
            // Allocate it
            if self.buffers[bucket].load(Ordering::Acquire).is_null() {
                self.allocate_bucket(bucket)
            }

            let last_elem = unsafe { &*self.get(current_desc.size) };

            let next_write_desc = WriteDescriptor::<T>::new_some_as_ptr(
                // TODO: address this in macro
                // # Safety
                // The `transmute_copy` is safe because we have ensured that T is the correct size at compile time
                unsafe { mem::transmute_copy::<T, u64>(&elem) },
                // Load from the AtomicU64, which really containes the bytes for T
                last_elem.load(Ordering::Acquire), 
                last_elem,
            );

            let next_desc = Descriptor::<T>::new_as_ptr(next_write_desc, current_desc.size + 1);

            if let Ok(replaced) = unsafe {
                HazAtomicPtr::compare_exchange_weak_ptr(
                    // # Safety
                    // Safe because the pointer we swap in points to a valid object that is !null
                    &self.descriptor,
                    current_desc as *const _ as *mut _,
                    next_desc,
                )
            } {
                self.complete_write(next_write_desc);

                // # Safety
                // Since the we only retire when swapping out a pointer, this is the only thread that will
                // retire, since only one thread receives the result of the swap (this one)
                //
                // There will never be another load call to the ptr because all calls will go the new one.
                // Since all uses of the inner wdesc are contained within the lifetime of the reference
                // to the desc, there will also be no new loads on the inner wdesc.
                unsafe {
                    replaced.unwrap().retire_in(&self.domain);
                }
                break;
            }

            // Deallocate the write_desc and desc that we failed to swap in
            // # Safety
            // Box the write_desc and desc ptrs were made from Box::into_raw, so it is safe to Box::from_raw
            unsafe {
                // Note: the inner wdesc also get's dropped as part of the desc's drop impl
                Box::from_raw(next_desc);
            }

            backoff.spin();
        }
    }

    pub fn pop(&self) -> Option<T> {
        let backoff = Backoff::new(); // Backoff causes significant speedup
        loop {
            let mut dhp = HazardPointer::new_in_domain(&self.domain);
            let current_desc = unsafe { self.descriptor.load(&mut dhp) }
                .expect("invalid ptr for descriptor in pop");

            // Use a block to make explicit that the use of the wdesc does not outlive the use of the desc.
            // This means that when the desc is dropped, there will be no references to the wdesc inside.
            // And we can deallocate the wdesc with `Box::from_raw`
            {
                let mut wdhp = HazardPointer::new_in_domain(&self.domain);
                let pending = unsafe { current_desc.pending.load(&mut wdhp) }
                    .expect("invalid ptr for write-descriptor in pop");

                self.complete_write(pending as *const _ as *mut _);
                // Hazard pointer is dropped, protection ends
            }

            if current_desc.size == 0 {
                return None;
            }

            // TODO: add safety comment
            // Consider if new desc is swapped in, can we read dealloced memory?
            // Do not need to worry about underflow for the sub because we would have already returned
            let elem = unsafe { &*self.get(current_desc.size - 1) }.load(Ordering::Acquire);

            let new_pending = WriteDescriptor::<T>::new_none_as_ptr();

            let next_desc = Descriptor::<T>::new_as_ptr(new_pending, current_desc.size - 1);

            if let Ok(replaced) = unsafe {
                HazAtomicPtr::compare_exchange_weak_ptr(
                    // # Safety
                    // Safe because the pointer we swap in points to a valid object that is !null
                    &self.descriptor,
                    current_desc as *const _ as *mut _,
                    next_desc,
                )
            } {
                // # Safety
                // Since the we only retire when swapping out a pointer, this is the only thread that will
                // retire, since only one thread receives the result of the swap (this one)
                //
                // There will never be another load call to the ptr because all calls will go the new one.
                // Since all uses of the inner wdesc are contained within the lifetime of the reference
                // to the desc, there will also be no new loads on the inner wdesc.
                unsafe {
                    replaced.unwrap().retire_in(&self.domain);
                }

                // # Safety 
                // TODO: address this in macro
                // This is ok because we ensure T is the correct size at compile time
                // We also know that elem is a valid T because it was transmuted into a usize
                // from a valid T, therefore we are only transmuting it back
                return Some(unsafe { mem::transmute_copy::<u64, T>(&elem) });
            }

            // Deallocate the write_desc and desc that we failed to swap in
            // # Safety
            // Box the write_desc and desc ptrs were made from Box::into_raw, so it is safe to Box::from_raw
            unsafe {
                // Note: the inner wdesc also get's dropped as part of the desc's drop impl
                Box::from_raw(next_desc);
            }

            backoff.spin();
        }
    }

    pub fn reserve(&self, size: usize) {
        // Cache the size to prevent another atomic op from due to calling `size()` again
        let current_size = self.size();
        if current_size == 0 {
            self.allocate_bucket(0);
        }

        // Number of allocations needed for current size
        let mut num_current_allocs =
            highest_bit(current_size.saturating_add(FIRST_BUCKET_SIZE) - 1)
                .saturating_sub(highest_bit(FIRST_BUCKET_SIZE));

        // Compare with the number of allocations needed for size `new`
        while num_current_allocs
            < highest_bit(size.saturating_add(FIRST_BUCKET_SIZE) - 1)
                .saturating_sub(highest_bit(FIRST_BUCKET_SIZE))
        {
            num_current_allocs += 1;
            self.allocate_bucket(num_current_allocs as usize);
        }
    }

    /// Return the size of the vector, taking into account a pending write operation
    /// ```rust
    /// # use unlocked::sealed::SecVec;
    /// let sv = SecVec::<isize>::new();
    /// sv.push(-1);
    /// sv.push(-2);
    /// sv.pop();
    /// assert_eq!(sv.size(), 1);
    /// ```
    pub fn size(&self) -> usize {
        let mut dhp = HazardPointer::new_in_domain(&self.domain);
        let desc = unsafe { self.descriptor.load(&mut dhp) }
            .expect("invalid pointer for descriptor in size");

        let size = desc.size;

        // If there is a pending descriptor, we subtract one from the size because
        // `push` increments the size, swaps the new descriptor in, and _then_ writes the value
        // Therefore the size is one greater because the write hasn't happened yet
        // # Safety
        // Descriptors available to multiple threads are always retired through &self.domain
        let mut wdhp = HazardPointer::new_in_domain(&self.domain);
        match unsafe { desc.pending.load(&mut wdhp) }
            .expect("invalid ptr reading write-desc in size")
        {
            Some(_) => size - 1,
            None => size,
        }
    }

    fn allocate_bucket(&self, bucket: usize) {
        // The shift-left is equivalent to raising 2 to the power of bucket
        let size = FIRST_BUCKET_SIZE * (1 << bucket);
        let layout = match Layout::array::<AtomicU64>(size) {
            Ok(layout) => layout,
            Err(_) => capacity_overflow(),
        };

        // Make sure allocation is ok
        match alloc_guard(layout.size()) {
            Ok(_) => {}
            Err(_) => capacity_overflow(),
        }

        let allocator = Global;

        // The reason for using allocate_zeroed is that miri complains about accessing uninitialized memory otherwise
        //
        // The situation is when we allocate the memory, and then try to CAS a new value in:
        // (AcqRel, Relaxed) => intrinsics::atomic_cxchg_acqrel_failrelaxed(dst, old, new),
        //                      ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ using uninitialized data, but this operation requires initialized memory
        // This shouldn't be an actual issue since the old value is never use, so might switch back to allocate (regular)
        // TODO: Maybe use MaybeUninit?
        let allocation = allocator.allocate_zeroed(layout);
        let ptr = match allocation {
            Ok(ptr) => ptr.as_ptr() as *mut AtomicU64,
            Err(_) => handle_alloc_error(layout),
        };

        // If the CAS fails, then the bucket has already been initalized with memory
        // and we free the memory we just allocated
        if self.buffers[bucket]
            .compare_exchange(
                ptr::null_mut::<AtomicU64>(),
                ptr,
                Ordering::AcqRel,
                Ordering::Relaxed,
            )
            .is_err()
        {
            unsafe {
                // # Safety
                // We know that the pointer returned from the allocation is NonNull
                // so we can call unwrap() on NonNull::new(). We also know that the pointer
                // is pointing to the correct memory because we just got it from the allocation.
                // We know the layout is valid, as it is the same layout we used to allocate.
                allocator.deallocate(NonNull::new(ptr as *mut u8).unwrap(), layout);
            }
        }
    }
}

impl<'a, T> Default for SecVec<'a, T>
where
    T: Copy + Sync + Send,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, T> Drop for SecVec<'_, T>
where
    T: Copy + Send + Sync,
{
    fn drop(&mut self) {
        // Drop buffers
        let allocator = Global;
        for (bucket, ptr) in self
            .buffers
            .iter()
            .filter(|ptr| !ptr.load(Ordering::Relaxed).is_null())
            .enumerate()
        // Getting all non-null buckets
        {
            let size = FIRST_BUCKET_SIZE * (1 << bucket);
            let layout = match Layout::array::<AtomicU64>(size) {
                Ok(layout) => layout,
                Err(_) => capacity_overflow(),
            };
            unsafe {
                // # Safety
                // We have recreated the exact same layout used to alloc the ptr in `allocate_bucket`
                // We know the ptr isn't null becase of the filer
                allocator.deallocate(
                    NonNull::new(ptr.load(Ordering::Relaxed) as *mut u8).unwrap(),
                    layout,
                )
            };
        }

        // Retiring the current desc and wdesc
        // # Safety
        // Since we have &mut self, we have exclusive access, so we can retire the desc and wdesc ptrs.
        // It is safe to deref the ptr to the desc because it is valid because it was created with
        // Descriptor::new_as_ptr.
        let desc = self.descriptor.load_ptr();
        unsafe {
            Box::from_raw(desc);
        };
    }
}

impl<'a, T> Drop for Descriptor<'_, T>
where
    T: Send,
{
    fn drop(&mut self) {
        // TODO: safety comment
        // Must ensure ref to wdesc never outlasts ref to desc
        unsafe {
            Box::from_raw(
                self.pending
                    .swap_ptr(ptr::null_mut())
                    .unwrap()
                    .into_inner()
                    .as_ptr(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_starts_at_0() {
        let sv = SecVec::<usize>::new();
        assert_eq!(0, sv.size());
    }

    #[test]
    fn pop_empty_returns_none() {
        let sv = SecVec::<usize>::new();
        assert_eq!(sv.pop(), None);
    }

    #[test]
    fn ten_push_ten_pop() {
        let sv = SecVec::<isize>::new();
        for i in 0..10 {
            sv.push(i);
        }
        for i in (0..10).rev() {
            assert_eq!(sv.pop(), Some(i));
        }
    }

    #[test]
    fn does_not_allocate_buffers_on_new() {
        let sv = SecVec::<isize>::new();
        for buffer in &**sv.buffers {
            assert!(buffer.load(Ordering::Relaxed).is_null())
        }
    }

    #[cfg(not(miri))] // Too slow
    #[test]
    #[should_panic] // The allocation is too large, will SIGABRT
    fn reserve_usize_max() {
        let sv = SecVec::<isize>::new();
        sv.reserve(usize::MAX)
    }
}