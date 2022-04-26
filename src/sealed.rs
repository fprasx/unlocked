/*extern crate alloc;
use crate::alloc_error::{alloc_guard, capacity_overflow};
use crate::highest_bit;
use alloc::alloc::{handle_alloc_error, Allocator, Global, Layout};
use alloc::boxed::Box;
use core::marker::PhantomData;
use core::mem::{self, ManuallyDrop};
use core::ptr::{self, NonNull};
use core::sync::atomic::{AtomicPtr, AtomicU64, Ordering};
use haphazard;
use crossbeam_utils::{Backoff, CachePadded};

// Setting up hazard pointers
// This makes sure they all use the same Domain, guaranteeing the protection is valid.
#[non_exhaustive]
struct Family;
type Domain = haphazard::Domain<Family>;
type HazardPointer<'domain> = haphazard::HazardPointer<'domain, Family>;
type HazAtomicPtr<T> = haphazard::AtomicPtr<T, Family>;

/// The number of elements in the first allocation.
/// Must always be a power of 2.
pub const FIRST_BUCKET_SIZE: usize = 8;

pub const ATOMIC_NULLPTR: AtomicPtr<AtomicU64> = AtomicPtr::new(ptr::null_mut::<AtomicU64>());

pub struct SecVec<'a, T: Sized> {
    buffers: CachePadded<Box<[AtomicPtr<AtomicU64>; 60]>>,
    descriptor: CachePadded<HazAtomicPtr<Descriptor<'a, T>>>,
    domain: Domain,
    // The data is technically stored as usizes, but it's really just transmuted T's
    _marker: PhantomData<T>,
}

struct Descriptor<'a, T: Sized> {
    pending: HazAtomicPtr<Option<WriteDescriptor<'a, T>>>,
    size: usize,
}

struct WriteDescriptor<'a, T: Sized> {
    new: u64,
    old: u64,
    location: &'a AtomicU64,
    // New and old are transmuted T's
    _marker: PhantomData<T>,
}

impl<'a, T> Descriptor<'a, T> {
    pub fn new(pending: *mut Option<WriteDescriptor<'a, T>>, size: usize) -> Self {
        Descriptor {
            // # Safety
            // This is safe because pending is always the result of calling WriteDescriptor::new_*_as_ptr
            // which used the pointer from Box::into_raw which is guaranteed to be valid.
            // Descriptors are only reclaimed through hazard pointer mechanisms
            pending: unsafe { HazAtomicPtr::new(pending) },
            size,
        }
    }

    pub fn new_as_ptr(
        pending: *mut Option<WriteDescriptor<'a, T>>,
        size: usize,
    ) -> *mut Self {
        Box::into_raw(Box::new(Descriptor::new(pending, size)))
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
        let descriptor = Descriptor::<T>::new_as_ptr(pending, 0);
        let buffers = Box::new([ATOMIC_NULLPTR; 60]);
        let domain = Domain::new(&Family {});
        Self {
            // Constructing HazAtomicPtr is safe because the *mut Descriptor came from Box::into_raw,
            // and descriptors can only be reclaimed through hazptr mechanisms
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
            // We know the offset will not go out of bounds because of the assert.
            buffer.load(Ordering::Acquire).add(offset)
        }
    }

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
            let next_desc = Descriptor::<T>::new_as_ptr(write_desc, current_desc.size + 1);
            // Handle result of compare_exchange
            if let Ok(old_ptr) = AtomicPtr::compare_exchange_weak(
                &self.descriptor,
                current_desc as *const _ as *mut _,
                next_desc,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                self.complete_write(unsafe { &*((*next_desc).pending.load(Ordering::Acquire)) });
                // TODO: remove this once we have a proper memory reclamation strategy
                // Manually drop prevents dealloc of `ptr` at end of scope
                let _wont_dealloc = ManuallyDrop::new(old_ptr);
                break;
            }
            backoff.spin();
        }
    }

    pub fn pop(&self) -> Option<T> {
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
            let new_pending = WriteDescriptor::<T>::new_none_as_ptr();
            let next_desc = Descriptor::<T>::new_as_ptr(new_pending, current_desc.size - 1);
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

// #[cfg(test)]
// mod tests {
//     use super::*;

//     #[test]
//     fn size_starts_at_0() {
//         let sv = SecVec::<usize>::new();
//         assert_eq!(0, sv.size());
//     }

//     #[test]
//     fn pop_empty_returns_none() {
//         let sv = SecVec::<usize>::new();
//         assert_eq!(sv.pop(), None);
//     }

//     #[test]
//     fn ten_push_ten_pop() {
//         let sv = SecVec::<isize>::new();
//         for i in 0..10 {
//             sv.push(i);
//         }
//         for i in (0..10).rev() {
//             assert_eq!(sv.pop(), Some(i));
//         }
//     }

//     #[test]
//     fn does_not_allocate_buffers_on_new() {
//         let sv = SecVec::<isize>::new();
//         for buffer in &**sv.buffers {
//             assert!(buffer.load(Ordering::Relaxed).is_null())
//         }
//     }

//     #[cfg(not(miri))] // Too slow
//     #[test]
//     #[should_panic] // The allocation is too large
//     fn reserve_usize_max() {
//         let sv = SecVec::<isize>::new();
//         sv.reserve(usize::MAX)
//     }
// }
// #[cfg(not(miri))] // Too slow
// #[cfg(test)]
// mod bench {
//     extern crate std;
//     extern crate test;
//     use super::*;
//     use crossbeam_queue::SegQueue;
//     use std::sync::Arc;
//     use std::sync::Mutex;
//     use std::thread::{self, JoinHandle};
//     use std::vec::Vec;
//     use test::Bencher;

//     macro_rules! queue {
//         ($($funcname:ident: $threads:expr),*) => {
//             $(
//                 #[bench]
//                 fn $funcname(b: &mut Bencher) {
//                     let sv = Arc::new(SegQueue::<isize>::new());
//                     b.iter(|| {
//                         #[allow(clippy::needless_collect)]
//                         let handles = (0..$threads)
//                             .map(|_| {
//                                 let data = Arc::clone(&sv);
//                                 thread::spawn(move || {
//                                     for i in 0..1000 {
//                                         data.push(i);
//                                     }
//                                 })
//                             })
//                             .collect::<Vec<JoinHandle<()>>>();
//                         handles.into_iter().for_each(|h| h.join().unwrap());
//                     });
//                 }
//                         )*
//         };
//     }

//     macro_rules! unlocked {
//         ($($funcname:ident: $threads:expr),*) => {
//             $(
//                 #[bench]
//                 fn $funcname(b: &mut Bencher) {
//                     let sv = Arc::new(SecVec::<isize>::new());
//                     sv.reserve(1000 * $threads);
//                     b.iter(|| {
//                         #[allow(clippy::needless_collect)]
//                         let handles = (0..$threads)
//                             .map(|_| {
//                                 let data = Arc::clone(&sv);
//                                 thread::spawn(move || {
//                                     for i in 0..1000 {
//                                         data.push(i);
//                                     }
//                                 })
//                             })
//                             .collect::<Vec<JoinHandle<()>>>();
//                         handles.into_iter().for_each(|h| h.join().unwrap());
//                     });
//                 }
//                         )*
//         };
//     }

//     macro_rules! mutex {
//         ($($funcname:ident: $threads:expr),*) => {
//             $(
//                 #[bench]
//                 fn $funcname(b: &mut Bencher) {
//                     let sv = Arc::new(Mutex::new(Vec::<isize>::with_capacity(1000 * $threads)));
//                     b.iter(|| {
//                         #[allow(clippy::needless_collect)]
//                         let handles = (0..$threads)
//                             .map(|_| {
//                                 let data = Arc::clone(&sv);
//                                 thread::spawn(move || {
//                                     for i in 0..1000 {
//                                         let mut g = data.lock().unwrap();
//                                         g.push(i);
//                                     }
//                                 })
//                             })
//                             .collect::<Vec<JoinHandle<()>>>();
//                         handles.into_iter().for_each(|h| h.join().unwrap());
//                     });
//                 }
//                         )*
//         };
//     }
//     unlocked!(unlocked1: 1, unlocked2: 2, unlocked3: 3, unlocked4: 4, unlocked5: 5, unlocked6: 6);
//     mutex!(mutex1: 1, mutex2: 2, mutex3: 3, mutex4: 4, mutex5: 5, mutex6: 6, mutex: 20);
//     queue!(q1: 1, q2: 2, q3: 3, q4: 4, q5: 5);
// }





*/