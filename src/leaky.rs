// TODO: add ZST check here/ or in macro
// TODO: convince compiler we know the size of T
// https://stackoverflow.com/questions/30330519/compile-time-generic-type-size-check
// https://github.com/rust-lang/rfcs/blob/master/text/2000-const-generics.md
// https://github.com/rust-lang/rust/issues/43408
extern crate alloc;
use crate::alloc_error::{alloc_guard, capacity_overflow};
use crate::highest_bit;
use alloc::alloc::{handle_alloc_error, Allocator, Global, Layout};
use alloc::boxed::Box;
use core::fmt::Debug;
use core::marker::PhantomData;
use core::mem::{self, ManuallyDrop};
use core::ptr::{self, NonNull};
use core::sync::atomic::{AtomicPtr, AtomicU64, Ordering};
use crossbeam_utils::{Backoff, CachePadded};

// TODO: use FisrtBucketSize trait (as long as it works with the macro)
/// The number of elements in the first allocation.
/// Must always be a power of 2.
pub const FIRST_BUCKET_SIZE: usize = 8;

/// An AtomicPtr containing a null-pointer to an AtomicU64
#[allow(clippy::declare_interior_mutable_const)] // We actually do want this to be copied
pub const ATOMIC_NULLPTR: AtomicPtr<AtomicU64> = AtomicPtr::new(ptr::null_mut::<AtomicU64>());

// TODO: make generic parameter N: the number of buckets
/// Things to talk about in documentation:
/// Structure
/// T: Copy bound because of internal usage of transmute_copy
/// Why no lazy allocation
///
/// A lock-free vector over `T: Copy` types that can be safely modified accross thread boundaries.
///
/// The vector is an implementation of the algorithm described in the paper _Lock-free Dynamically
/// Resizable Arrays_ by **Dechev et. al.**, 2006.
///
/// # Uses
///
/// A concurrent stack that isn't a linked list. Mic Drop
///
/// Note: this vector cannot tbe used with ZST's. There is probably a much better way to do what you want.
/// If you just want to push/pop, you can use an atomic variable to track how many are left on the stack
/// and then just increment and decrement it.
///
/// # Considerations
///
/// This vector also uses dynamic allocation heavily. Internal data is allocated on the heap
/// because memory needs to be reclaimed in a sound way. Calling `new()` requires 3 heap allocations.
/// The first is just to allocate enough space for the vector's internal data. It is never called
/// again. The other two allocations set the state of the vector, and similar allocations are made
/// when pushing and popping.
///
/// The vector does not allocate lazily.
/// Checking whether the vector has already allocated is very expensive (even in a single-threaded
/// environment, at least one atomic read and compare_exchange), and would incur overhead on all
/// subsequent operations.
///
/// The size of the type is two pointers (16 bytes on 64-bit platforms), but the vector allocates 480
/// bytes of heap memory upfront. Bear this in mind if you are in a memory constrained environment.
///
/// This vector does not support types larger than usize because it uses atomic instructions internally.
/// Larger types must be accessed through references/pointers.
///
/// # Internal structure
///
/// Internally, the vector stores elements in buckets, which grow the same way allocations
/// in a normal vector do. Thus, the first bucket might have size 8, the next size 16, then 32, etc.
///
/// The vector relies heavily on the `compare_exchange` instruction to achieve synchronization.
///
/// Memory reclamation is achieved through the use of hazard pointers.
///
#[derive(Debug)]
pub struct SecVec<'a, T: Sized> {
    // TODO: are we going to have a false sharing problem?
    // Could use a wrapper type if so
    // See: https://github.com/Amanieu/atomic-rs/blob/master/src/fallback.rs#L21
    buffers: CachePadded<Box<[AtomicPtr<AtomicU64>; 60]>>,
    descriptor: CachePadded<AtomicPtr<Descriptor<'a, T>>>,
    // The data is technically stored as usizes, but it's really just transmuted T's
    _marker: PhantomData<T>,
}

/// TODO: add docs
struct Descriptor<'a, T: Sized> {
    pending: AtomicPtr<Option<WriteDescriptor<'a, T>>>,
    size: usize,
    // For reference counting?
    // TODO: figure out memory reclamation scheme, would be AtomicU64 in that case
    _counter: usize,
}

/// TODO: add docs
/// Both new and old are just  T's transmuted into usize, thus the PhantomData
struct WriteDescriptor<'a, T: Sized> {
    new: u64,
    old: u64,
    location: &'a AtomicU64,
    _marker: PhantomData<T>,
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
    pub fn new(new: u64, old: u64, location: &'a AtomicU64) -> Self {
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

    pub fn new_some_as_ptr(new: u64, old: u64, location: &'a AtomicU64) -> *mut Option<Self> {
        Box::into_raw(Box::new(Some(WriteDescriptor::new(new, old, location))))
    }
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
            descriptor: CachePadded::new(AtomicPtr::new(descriptor)),
            buffers: CachePadded::new(buffers),
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
            // We know the offset will not go out of bounds because of the assert.
            buffer.load(Ordering::Acquire).add(offset)
        }
    }

    /// 1. Check if there is a writeop (write-descriptor) pending
    /// 2. If so, CAS the location in the buffer with the new value
    /// 3. Set the writeop state to false
    fn complete_write(&self, pending: &Option<WriteDescriptor<T>>) {
        #[allow(unused_must_use)]
        if let Some(writedesc) = pending {
            AtomicU64::compare_exchange(
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
        let backoff = Backoff::new(); // Backoff causes significant speedup
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
                unsafe { mem::transmute_copy::<T, u64>(&elem) }, // SAFE because we know T has correct size
                last_elem.load(Ordering::Acquire), // Load from the AtomicU64, which really containes the bytes for T
                last_elem,
            );
            let next_desc = Descriptor::<T>::new_as_ptr(write_desc, current_desc.size + 1, 0);
            // Handle result of compare_exchange
            if AtomicPtr::compare_exchange_weak(
                &self.descriptor,
                current_desc as *const _ as *mut _,
                next_desc,
                Ordering::AcqRel,
                Ordering::Relaxed,
            )
            .is_ok()
            {
                // We know the current write_desc is the one we just sent in with the compare_exchange
                // so avoid loading it atomically
                self.complete_write(unsafe { &*write_desc });
                break;
            }
            backoff.spin();
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
        let backoff = Backoff::new(); // Backoff causes significant speedup
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
                return Some(unsafe { mem::transmute_copy::<u64, T>(&elem) });
            }
            backoff.spin();
        }
    }

    // ============================================================
    // ========================== MEMORY ==========================
    // ============================================================
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

    /// TODO: add panic documentation
    /// TODO: figure out overflow docs
    /// Reserve enough space for the provided number of elements.
    ///
    /// You should always call this if you know how many elements you will need in advance
    /// because allocation requires a CAS and it's better to do it while there's less
    /// contention.
    ///
    /// Note: if you call reserve with a value larger than the capacity of the vector,
    /// the vector will allocate as much as it can.
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
                // SAFETY
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
        for buffer in &**sv.buffers {
            assert!(buffer.load(Ordering::Relaxed).is_null())
        }
    }

    #[cfg(not(miri))] // Too slow
    #[test]
    #[should_panic] // The allocation is too large
    fn reserve_usize_max() {
        let sv = SecVec::<isize>::new();
        sv.reserve(usize::MAX)
    }
}
#[cfg(not(miri))] // Too slow
#[cfg(test)]
mod bench {
    extern crate std;
    extern crate test;
    use super::*;
    use crossbeam_queue::SegQueue;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::thread::{self, JoinHandle};
    use std::vec::Vec;
    use test::Bencher;

    macro_rules! queue {
        ($($funcname:ident: $threads:expr),*) => {
            $(
                #[bench]
                fn $funcname(b: &mut Bencher) {
                    let sv = Arc::new(SegQueue::<isize>::new());
                    b.iter(|| {
                        #[allow(clippy::needless_collect)]
                        let handles = (0..$threads)
                            .map(|_| {
                                let data = Arc::clone(&sv);
                                thread::spawn(move || {
                                    for i in 0..1000 {
                                        data.push(i);
                                    }
                                })
                            })
                            .collect::<Vec<JoinHandle<()>>>();
                        handles.into_iter().for_each(|h| h.join().unwrap());
                    });
                }
                        )*
        };
    }

    macro_rules! unlocked {
        ($($funcname:ident: $threads:expr),*) => {
            $(
                #[bench]
                fn $funcname(b: &mut Bencher) {
                    let sv = Arc::new(SecVec::<isize>::new());
                    sv.reserve(1000 * $threads);
                    b.iter(|| {
                        #[allow(clippy::needless_collect)]
                        let handles = (0..$threads)
                            .map(|_| {
                                let data = Arc::clone(&sv);
                                thread::spawn(move || {
                                    for i in 0..1000 {
                                        data.push(i);
                                    }
                                })
                            })
                            .collect::<Vec<JoinHandle<()>>>();
                        handles.into_iter().for_each(|h| h.join().unwrap());
                    });
                }
                        )*
        };
    }

    macro_rules! mutex {
        ($($funcname:ident: $threads:expr),*) => {
            $(
                #[bench]
                fn $funcname(b: &mut Bencher) {
                    let sv = Arc::new(Mutex::new(Vec::<isize>::with_capacity(1000 * $threads)));
                    b.iter(|| {
                        #[allow(clippy::needless_collect)]
                        let handles = (0..$threads)
                            .map(|_| {
                                let data = Arc::clone(&sv);
                                thread::spawn(move || {
                                    for i in 0..1000 {
                                        let mut g = data.lock().unwrap();
                                        g.push(i);
                                    }
                                })
                            })
                            .collect::<Vec<JoinHandle<()>>>();
                        handles.into_iter().for_each(|h| h.join().unwrap());
                    });
                }
                        )*
        };
    }
    unlocked!(unlocked1: 1, unlocked2: 2, unlocked3: 3, unlocked4: 4, unlocked5: 5, unlocked6: 6);
    mutex!(mutex1: 1, mutex2: 2, mutex3: 3, mutex4: 4, mutex5: 5, mutex6: 6, mutex: 20);
    queue!(q1: 1, q2: 2, q3: 3, q4: 4, q5: 5);
}
