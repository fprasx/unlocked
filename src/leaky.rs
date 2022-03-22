// TODO: figure out semantics from drop with values in SecVec, since transmute_copy makes a copy
// Solution: just add a T: Copy bound?
// TODO: convince compiler we know the size of T
// https://stackoverflow.com/questions/30330519/compile-time-generic-type-size-check
// https://github.com/rust-lang/rfcs/blob/master/text/2000-const-generics.md
// https://github.com/rust-lang/rust/issues/43408
extern crate alloc;
use crate::highest_bit;
use alloc::alloc::{Allocator, Global};
use alloc::boxed::Box;
use core::alloc::Layout;
use core::fmt::Debug;
use core::marker::PhantomData;
use core::mem::{self, ManuallyDrop};
use core::ptr::{self, null_mut};
use core::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

/// The number of elements in the first allocation.
/// Must always be a power of 2.
pub const FIRST_BUCKET_SIZE: usize = 8;

/// An AtomicPtr containing a null-pointer to an AtomicUsize
#[allow(clippy::declare_interior_mutable_const)] // We actually do want this to be copied
pub const ATOMIC_NULLPTR: AtomicPtr<AtomicUsize> = AtomicPtr::new(ptr::null_mut::<AtomicUsize>());

// TODO: make generic parameter N: the number of buckets
/// Things to talk about in documentation:
/// Structure
/// T: Copy bound because of internal usage of transmute_copy
/// Why no lazy allocation
#[derive(Debug)]
pub struct SecVec<'a, T: Sized> {
    // TODO: are we going to have a false sharing problem?
    // Could use a wrapper type if so
    // See: https://github.com/Amanieu/atomic-rs/blob/master/src/fallback.rs#L21
    buffers: Box<[AtomicPtr<AtomicUsize>; 60]>,
    descriptor: AtomicPtr<Descriptor<'a, T>>,
    // The data is technically stored as usizes, but it's really just transmuted T's
    _marker: PhantomData<T>,
}

/// TODO: add docs
#[derive(Debug)]
struct Descriptor<'a, T: Sized> {
    pending: AtomicPtr<Option<WriteDescriptor<'a, T>>>,
    size: usize,
    // For reference counting?
    // TODO: figure out memory reclamation scheme, would be AtomicUsize in that case
    _counter: usize,
}

/// TODO: add docs
/// Both new and old are just  T's transmuted into usize, thus the PhantomData
#[derive(Debug)]
struct WriteDescriptor<'a, T: Sized> {
    new: usize,
    old: usize,
    location: &'a AtomicUsize,
    _marker: PhantomData<T>,
}

impl<'a, T> SecVec<'a, T>
where
    T: Sized + Copy,
{
    /// Return of new instance of a SecVec, with capacity 0 and size 0;
    pub fn new() -> Self {
        let pending = WriteDescriptor::<T>::new_none_as_ptr();
        let descriptor = Descriptor::<T>::new_as_ptr(pending, 0, 0);
        let buffers = Box::new([ATOMIC_NULLPTR; 60]);
        Self {
            descriptor: AtomicPtr::new(descriptor),
            buffers,
            _marker: PhantomData,
        }
    }

    #[deny(unsafe_op_in_unsafe_fn)]
    /// Return a *const T to the index specified
    ///
    /// # Safety
    /// The index this is called on **must** be a valid index, meaning:
    /// there must already be a bucket allocated which would hold that index
    /// **and** the index must already have been initialized with push/set
    unsafe fn get(&self, i: usize) -> *const AtomicUsize {
        // Technically this could overflow!
        // HOWEVER, it is extremely unlikely that `self` would be holding anywhere close usize::MAX elements
        // As that is 18 exabytes of memory
        let pos = i.checked_add(FIRST_BUCKET_SIZE).expect("index too large, integer overlow");
        let hibit = highest_bit(pos);
        // The shift-left is 2 to the power of hibit
        let index = pos ^ (1 << hibit);
        // Select the correct buffer to index into
        // # Safety
        // Since hibit = highest_bit(pos), and pos >= FIRST_BUCKET_SIZE
        // The subtraction hibit - highest_bit(FIRST_BUCKET_SIZE) cannot underflow
        let buffer = &self.buffers[(hibit - highest_bit(FIRST_BUCKET_SIZE)) as usize];
        // Offset the pointer to return a pointer to the correct element
        unsafe {
            // # Safety
            // We know that we can offset the pointer because we will have allocated a bucket
            // to store the value. Since we only call values that are `self.descriptor.size` or smaller,
            // We know the offset will not go out of bounds.
            // Also this function is not part of the public API
            // 
            // On overflowing:
            // The max number of elements the vector can hold is usize::MAX,
            // and the last bucket holds exactly half that many, 2 ** 63, and isize::MAX is 2 **63 - 1.
            // From the first statement in this function, we know that i < usize::MAX
            // Therefore index cannot overflow an isize because the largest bucket holds isize::MAX + 1
            // and i < usize::MAX, so the last FIRST_BUCKET_SIZE elements of the last bucket will
            // not be touched
            //
            // UNLESS: FIRST_BUCKET_SIZE is 0 for zst's, but that's a whole different thing
            // and the vector currently doesn't support zst's
            buffer.load(Ordering::Acquire).add(index)
        }
    }

    /// 1. Check if there is a writeop (write-descriptor) pending
    /// 2. If so, CAS the location in the buffer with the new value
    /// 3. Set the writeop state to false
    fn complete_write(&self, pending: &Option<WriteDescriptor<T>>) {
        #[allow(unused_must_use)]
        if let Some(writedesc) = pending {
            AtomicUsize::compare_exchange(
                writedesc.location,
                writedesc.old,
                writedesc.new,
                Ordering::AcqRel,
                Ordering::Relaxed,
            );
            let new_writedesc = WriteDescriptor::<T>::new_none_as_ptr();
            // # Safety
            // The pointer is valid to dereference because it started off valid and only pointers made from
            // from Descriptor::new_as_ptr() (which are valid because of Box) are CAS'd in
            //
            // The success of the CAS also doesn't matter, if the CAS failed, that means that another thread
            // beat us to the write. Thus, in `push()`, we'll simply load in the new descriptor (this one),
            // and proceed. Acquire/Release semantics guarantee that the next loop iteration will see this new write descriptor
            unsafe { &*self.descriptor.load(Ordering::Acquire) }
                .pending
                .store(new_writedesc, Ordering::Release);
        }
    }

    pub fn push(&self, elem: T) {
        /*
        * 1. Pull down the current descriptor
        * 2. Call complete_write on it to clear out a pending writeop
        * 3. Allocate memory if need be
        * 4. Create a new write-descriptor
        * 5. Try to CAS in the new write-descriptor
        * 6. Go back to step 1 if CAS failed
        * 7. Call complete_write to finish the write
        */

        loop {
            // # SAFETY
            // It is safe to dereference the raw pointer because the first descriptor was valid
            // and all other descriptors are valid descriptors that were CAS'd in
            let current_desc = unsafe { &*self.descriptor.load(Ordering::Acquire) };
            // Complete a pending write op if there is any
            // # SAFETY
            // It is safe to dereference the raw pointer because the first descriptor was valid
            // and all other descriptors are valid descriptors that were CAS'd in
            let pending = unsafe { &*current_desc.pending.load(Ordering::Acquire) };
            self.complete_write(pending);
            // Allocate memory if need be
            let bucket = (highest_bit(current_desc.size + FIRST_BUCKET_SIZE)
                - highest_bit(FIRST_BUCKET_SIZE)) as usize;
            if self.buffers[bucket].load(Ordering::Acquire).is_null() {
                self.allocate_bucket(bucket)
            }

            // # SAFETY
            // It is safe to dereference the raw pointer because we made sure to allocate
            // memory previously, so it is pointing into valid memory
            let last_elem = unsafe { &*self.get(current_desc.size) };
            let write_desc = WriteDescriptor::<T>::new_some_as_ptr(
                // TODO: address this in macro
                unsafe { mem::transmute_copy::<T, usize>(&elem) }, // SAFE because we know T has correct size
                last_elem.load(Ordering::Acquire), // Load from the AtomicUsize, which really containes the bytes for T
                last_elem,
            );
            let next_desc = Descriptor::<T>::new_as_ptr(write_desc, current_desc.size + 1, 0);
            // Debugging next_desc
            match AtomicPtr::compare_exchange_weak(
                &self.descriptor,
                current_desc as *const _ as *mut _,
                next_desc,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(old_ptr) => {
                    self.complete_write(unsafe {
                        &*((*next_desc).pending.load(Ordering::Acquire))
                    });
                    // TODO: remove this once we have a proper memory reclamation strategy
                    // Manually drop prevents dealloc of `ptr` at end of scope
                    let _wont_dealloc = ManuallyDrop::new(old_ptr);
                    break;
                }
                Err(_) => continue,
            }
        }
    }

    pub fn pop(&self) -> Option<T> {
        /*
        1. Pull down the current descriptor
        2. Call complete_write on it to clear out a pending writeop
        3. Read in the element at the end of the array
        4. Make a new descriptor
        5. Try to CAS in the new descriptor
        6. Go back to step 1 if CAS failed
        7. Return the element that was read in from the end of the array
        */
        loop {
            let current_desc = unsafe { &*self.descriptor.load(Ordering::Acquire) };
            let pending = unsafe { &*current_desc.pending.load(Ordering::Acquire) };
            self.complete_write(pending);
            if current_desc.size == 0 {
                return None;
            }
            // #
            // Do not need to worry about underflow for the sub because we would hav already return
            let elem = unsafe { &*self.get(current_desc.size - 1) }.load(Ordering::Acquire);
            // BUG LOG
            // let next_desc = Box::into_raw(Box::new(Descriptor::<T> {
            //     size: current_desc.size - 1,
            //     pending: AtomicPtr::new(&mut None as *mut Option<WriteDescriptor<T>>),
            //     counter: 0,
            // }));
            //
            // There was a use-after-free caused by the &mut None being turned into a raw ptr
            // because the ptr's mem was deallocated when the function returned and the stack frame was destroyed
            let new_pending = WriteDescriptor::<T>::new_none_as_ptr();
            let next_desc = Descriptor::<T>::new_as_ptr(new_pending, current_desc.size - 1, 0);
            if AtomicPtr::compare_exchange_weak(
                &self.descriptor,
                current_desc as *const _ as *mut _,
                next_desc,
                Ordering::AcqRel,
                Ordering::Relaxed,
            )
            .is_ok()
            {
                // SAFETY
                // This is ok because only 64-bit values can be stored in the vector
                // We also know that elem is a valid T because it was transmuted into a usize
                // from a valid T, therefore we are only transmuting it back
                return Some(unsafe { mem::transmute_copy::<usize, T>(&elem) });
            }
        }
    }

    // *** MEMORY ***
    // Sadly, the vector does not currently allocate lazily
    // meaning the overhead from storing an array of buckets
    // is always there
    //
    // If we wanted to allocate lazily, we would need to check the
    // size everytime we push, which is expenseive because we
    // need to follow a pointer to the descriptor and then check
    // if there write descriptor
    //
    // TODO: It could still faster though, should benchmark it out

    /// Reserve enough space for the provided number of elements.
    /// 
    /// You should always call this if you know how many elements you will need in advance
    /// because allocation requires a CAS and it's better to do it while there's less 
    /// contention.
    /// 
    /// ```rust
    /// # use unlocked::leaky::SecVec;
    /// # use std::sync::Arc;
    /// # use std::thread;
    /// let sv = Arc::new(SecVec::<isize>::new());
    /// // Better to CAS there than in a thread
    /// sv.reserve(10);
    /// let sv1 = Arc::clone(&sv);
    /// let t1 = thread::spawn(move || {
    ///     for _ in 0..5 {
    ///         sv.push(5);
    ///     }
    /// });
    /// let t2 = thread::spawn(move || {
    ///     for _ in 0..5 {
    ///         sv1.push(5);
    ///     }
    /// });
    /// // We know that no allocations happened during the multi-threaded part
    /// t1.join().unwrap();
    /// t2.join().unwrap();
    /// ```
    pub fn reserve(&self, size: usize) {
        // Method
        // Calculate the number of buckets needed and their indices,
        // For each bucket, call allocate_bucket to reserve memory.
        // A slight problem is that the strategy for calculating which bucket
        // we are on cannot distinguish between 0 and anything between 1-7.
        // Therefore, we manually check if the size is 0 and allocate the first
        // bucket, then proceed to allocate the rest

        // Cache the size to prevent another atomic op from due to calling `size()` again
        let current_size = self.size();
        if current_size == 0 {
            self.allocate_bucket(0);
        }
        // Number of allocations needed for current size
        let mut num_current_allocs = highest_bit(current_size + FIRST_BUCKET_SIZE - 1)
            .saturating_sub(highest_bit(FIRST_BUCKET_SIZE));
        // Compare with the number of allocations needed for size `new`
        while num_current_allocs
            < highest_bit(size + FIRST_BUCKET_SIZE - 1)
                .saturating_sub(highest_bit(FIRST_BUCKET_SIZE))
        {
            num_current_allocs += 1;
            self.allocate_bucket(num_current_allocs as usize);
        }
    }

    /// Return the size of the vector, taking into account a pending write operation
    /// ```rust
    /// # use unlocked::leaky::SecVec;
    /// let sv = SecVec::<isize>::new();
    /// sv.push(-1);
    /// sv.push(-2);
    /// sv.pop();
    /// assert_eq!(sv.size(), 1);
    /// ```
    pub fn size(&self) -> usize {
        // # Safety
        // We know that the raw pointer is pointing to a valid descriptor
        // Because the vector started with a valid instance and the only
        // changes to vector.descriptor are through CAS'ing with another valid descriptor
        let desc = unsafe { &*self.descriptor.load(Ordering::Acquire) };
        let size = desc.size;
        // # Safety
        // We know that the raw pointer is pointing to a valid writedescriptor
        // Because the vector started with a valid writedescriptor
        // and changes can only be made through CAS'ing with another valid writedescriptor 
        //
        // If there is a pending descriptor, we subtract one from the size because
        // `push` increments the size, swaps the new descriptor in, and _then_ writes the value
        // Therefore the size is one greater because the write hasn't happened yet
        match unsafe { &*desc.pending.load(Ordering::Acquire) } {
            Some(_) => size - 1,
            None => size,
        }
    }

    /// Allocate the desired bucket from ```self.buffers```
    ///
    /// Steps:
    /// 1. Calculate the amount of memory needed for the bucket
    /// 2. Allocate the memory
    /// 3. Try to CAS in the pointer from the allocation.
    /// If the pointer in self.buffers is currently null, we know that it
    /// has not been initalized with memory, and the CAS will succeed. If
    /// CAS fails, then we know the bucket has already been initalized.
    /// 4. If CAS failed, deallocate the memory from Step 2
    fn allocate_bucket(&self, bucket: usize) {
        // The shift-left is equivalent to raising 2 to the power of bucket
        let size = FIRST_BUCKET_SIZE * (1 << bucket);
        let layout = Layout::array::<AtomicUsize>(size).expect("Size overflowed");
        let allocator = Global;
        // The reason for using allocate_zeroed is the miri complains about accessing uninitialized memory otherwise
        //
        // The situation is when we allocate the memory, and then try to CAS a new value in:
        // (AcqRel, Relaxed) => intrinsics::atomic_cxchg_acqrel_failrelaxed(dst, old, new),
        //                      ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ using uninitialized data, but this operation requires initialized memory
        // This shouldn't be an actual issue since the old value is never use, so might switch back to allocate (regular)
        // TODO: Maybe use MaybeInit?
        let ptr = allocator
            .allocate_zeroed(layout)
            .expect("Out of memory")
            .as_ptr() as *mut AtomicUsize;
        // If the CAS fails, then the bucket has already been initalized with memory
        // and we free the memory we just allocated
        if self.buffers[bucket]
            .compare_exchange(
                ptr::null_mut::<AtomicUsize>(),
                ptr,
                Ordering::AcqRel,
                Ordering::Relaxed,
            )
            .is_err()
        {
            unsafe {
                // SAFETY
                // We know that the pointer returned from the allocation is NonNull
                // so we can call unwrap() on NonNull::new(). We also know that the pointer
                // is pointing to the correct memory because we just got it from the allocation.
                // We know the layout is valid, as it is the same layout we used to allocate.
                allocator.deallocate(ptr::NonNull::new(ptr as *mut u8).unwrap(), layout);
            }
        }
    }
}

impl<'a, T> Descriptor<'a, T> {
    pub fn new(pending: *mut Option<WriteDescriptor<'a, T>>, size: usize, _counter: usize) -> Self {
        Descriptor {
            pending: AtomicPtr::new(pending),
            size,
            _counter,
        }
    }

    pub fn new_as_ptr(
        pending: *mut Option<WriteDescriptor<'a, T>>,
        size: usize,
        counter: usize,
    ) -> *mut Self {
        Box::into_raw(Box::new(Descriptor::new(pending, size, counter)))
    }
}

impl<'a, T> WriteDescriptor<'a, T> {
    pub fn new(new: usize, old: usize, location: &'a AtomicUsize) -> Self {
        WriteDescriptor {
            new,
            old,
            location,
            _marker: PhantomData::<T>,
        }
    }

    pub fn new_none_as_ptr() -> *mut Option<Self> {
        Box::into_raw(Box::new(None))
    }

    pub fn new_some_as_ptr(new: usize, old: usize, location: &'a AtomicUsize) -> *mut Option<Self> {
        Box::into_raw(Box::new(Some(WriteDescriptor::new(new, old, location))))
    }
}

impl<'a, T> Default for SecVec<'a, T>
where
    T: Copy,
{
    fn default() -> Self {
        Self::new()
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
        for buffer in *sv.buffers {
            assert!(buffer.load(Ordering::Relaxed).is_null())
        }
    }

    #[test]
    fn reserve_usize_max() {
        () 
    }

}
