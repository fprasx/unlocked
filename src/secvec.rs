// TODO: convince compiler we know the size of T
// https://stackoverflow.com/questions/30330519/compile-time-generic-type-size-check
// https://github.com/rust-lang/rfcs/blob/master/text/2000-const-generics.md
// https://github.com/rust-lang/rust/issues/43408
use crate::highest_bit;
use core::alloc::Layout;
use core::marker::PhantomData;
use core::ptr::*;
use core::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};
use std::alloc::{Allocator, Global};

pub const FIRST_BUCKET_SIZE: usize = 8;

/// An AtomicPtr containing a null-pointer to an AtomicUsize
#[allow(clippy::declare_interior_mutable_const)] // We actually do want this to be copied
pub const ATOMIC_NULLPTR: AtomicPtr<AtomicUsize> =
    AtomicPtr::new(std::ptr::null_mut::<AtomicUsize>());

// TODO: make generic parameter N: the number of buckets
pub struct SecVec<'a, T: Sized> {
    // Enough space to hold usize::MAX elements
    // Using array because growing a slice/vector might require more synchronization
    // which kinda defeats the whole lock-free part
    // However, this IS a HUGE storage overhead to consider; 480 bytes
    buffers: [AtomicPtr<AtomicUsize>; 60],
    descriptor: AtomicPtr<Descriptor<'a, T>>,
    // The data is technically stored as usizes, but it's really just transmuted T's
    _marker: PhantomData<T>,
}

struct Descriptor<'a, T: Sized> {
    // This pointer doesn't need to be atomic as
    // pending writes are CAS'd in so duplicates won't happen
    pending: AtomicPtr<Option<WriteDescriptor<'a, T>>>,
    size: usize,
    // For reference counting?
    counter: usize,
}

struct WriteDescriptor<'a, T: Sized> {
    new: usize,
    old: usize,
    location: &'a AtomicUsize,
    _marker: PhantomData<T>,
}

impl<'a, T> SecVec<'a, T>
where
    T: Sized,
{
    // TODO: add lazy allocation
    // Maybe use NonNull::dangling()
    pub fn new() -> Self {
        // This reference is never used again
        let write_desc: *mut Option<WriteDescriptor<T>> =
            &Option::<WriteDescriptor<T>>::None as *const _ as *mut _;
        let descriptor = &Descriptor::<T> {
            pending: AtomicPtr::new(write_desc),
            size: 0,
            counter: 0,
        } as *const Descriptor<T> as *mut Descriptor<T>;
        let buffers = [ATOMIC_NULLPTR; 60];
        Self {
            descriptor: AtomicPtr::new(descriptor),
            buffers,
            _marker: PhantomData,
        }
    }
    // Return a *const T to the index specified
    pub fn get(&self, i: usize) -> *const AtomicUsize {
        let pos = i + FIRST_BUCKET_SIZE;
        // The highest bit set in pos
        let hibit = highest_bit(pos);
        let index = pos ^ 2usize.pow(hibit);
        // Select the correct buffer to index into
        let buffer = &self.buffers[(hibit - highest_bit(FIRST_BUCKET_SIZE)) as usize];
        unsafe {
            // Offset the pointer to return a pointer to the correct element
            // SAFETY
            // We know that we can offset the pointer because we will have allocated a bucket
            // to store the value
            // And this function is not part of the public API
            // TODO: ensure that the index does not overflow
            buffer.load(Ordering::Acquire).add(index)
        }
    }

    fn complete_write(&self, pending: &Option<WriteDescriptor<T>>) {
        /*
        1. Check if there is a writeop (write-descriptor) pending
        2. If so, CAS the location in the buffer with the new value
        3. Set the writeop state to false
        */
        if let Some(writedesc) = pending {
            match AtomicUsize::compare_exchange(
                writedesc.location,
                writedesc.old,
                writedesc.new,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                _ => (),
            }
        }
    }

    pub fn push(&self, elem: T) {
        /*
        1. Pull down the current descriptor
        2. Call complete_write on it to clear out a pending writeop
        3. Allocate memory if need be
        4. Create a new write-descriptor
        5. Try to CAS in the new write-descriptor
        6. Go back to step 1 if CAS failed
        7. Call complete_write to finish the write
         */

        let mut current_desc = unsafe { &*self.descriptor.load(Ordering::Acquire) };
        // Complete a pending write op if there is anu
        let mut pending = unsafe { &*current_desc.pending.load(Ordering::Acquire) };
        self.complete_write(pending);
        // Allocate memory if need be
        let mut bucket = (highest_bit(current_desc.size + FIRST_BUCKET_SIZE)
            - highest_bit(FIRST_BUCKET_SIZE)) as usize;
        if self.buffers[bucket].load(Ordering::Acquire) == std::ptr::null_mut::<AtomicUsize>() {
            self.allocate_bucket(bucket)
        }
        // Make a new WriteDescriptor
        let mut last_elem_ptr = unsafe { &*self.get(current_desc.size) };
        let mut write_desc: Option<WriteDescriptor<T>> = Some(WriteDescriptor {
            old: last_elem_ptr.load(Ordering::Acquire),
            new: unsafe { std::mem::transmute_copy::<T, usize>(&elem) },
            // TODO: see if we can just use a reference
            location: last_elem_ptr,
            _marker: PhantomData,
        });
        let mut next_desc = Descriptor::<T> {
            pending: AtomicPtr::new(&write_desc as *const _ as *mut _),
            size: current_desc.size,
            counter: 0,
        };
        while AtomicPtr::compare_exchange(
            &self.descriptor,
            current_desc as *const _ as *mut _,
            &next_desc as *const _ as *mut _,
            Ordering::AcqRel,
            Ordering::Relaxed,
        )
        .is_err()
        {
            current_desc = unsafe { &*self.descriptor.load(Ordering::Acquire) };
            // Complete a pending write op if there is anu
            pending = unsafe { &*current_desc.pending.load(Ordering::Acquire) };
            self.complete_write(pending);
            // Allocate memory if need be
            bucket = (highest_bit(current_desc.size + FIRST_BUCKET_SIZE)
                - highest_bit(FIRST_BUCKET_SIZE)) as usize;
            if self.buffers[bucket].load(Ordering::Acquire) == std::ptr::null_mut::<AtomicUsize>() {
                self.allocate_bucket(bucket)
            }
            // Make a new WriteDescriptor
            last_elem_ptr = unsafe { &*self.get(current_desc.size) };
            // TODO: see if we can just use a reference
            write_desc = Some(WriteDescriptor {
                old: last_elem_ptr.load(Ordering::Acquire),
                new: unsafe { std::mem::transmute_copy::<T, usize>(&elem) },
                location: last_elem_ptr,
                _marker: PhantomData,
            });
            next_desc = Descriptor::<T> {
                pending: AtomicPtr::new(&write_desc as *const _ as *mut _),
                size: current_desc.size,
                counter: 0,
            };
        }
        self.complete_write(unsafe { &*next_desc.pending.load(Ordering::Acquire) })
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
        let mut current_desc = unsafe { &*self.descriptor.load(Ordering::Acquire) };
        let mut pending = unsafe { &*current_desc.pending.load(Ordering::Acquire) };
        self.complete_write(pending);
        // If this weren't here, we would panic on subtracting one from 0
        // Also this is the logically correct thing to do
        if current_desc.size == 0 {
            return None;
        }
        let mut elem = unsafe { &*self.get(current_desc.size - 1) }.load(Ordering::Acquire);
        let mut next_desc = Descriptor::<T> {
            size: current_desc.size,
            pending: AtomicPtr::new(&mut None as *mut Option<WriteDescriptor<T>>),
            counter: 0,
        };
        while AtomicPtr::compare_exchange(
            &self.descriptor,
            current_desc as *const _ as *mut _,
            &next_desc as *const _ as *mut _,
            Ordering::AcqRel,
            Ordering::Relaxed,
        )
        .is_err()
        {
            current_desc = unsafe { &*self.descriptor.load(Ordering::Acquire) };
            pending = unsafe { &*current_desc.pending.load(Ordering::Acquire) };
            self.complete_write(pending);
            elem = unsafe { &*self.get(current_desc.size - 1) }.load(Ordering::Acquire);
            next_desc = Descriptor::<T> {
                size: current_desc.size,
                pending: AtomicPtr::new(&mut None as *mut Option<WriteDescriptor<T>>),
                counter: 0,
            };
        }
        // SAFETY
        // This is ok because only 64-bit values can be stored in the vector
        // We also know that elem is a valid T because it was transmuted into a usize
        // from a valid T, therefore we are only transmuting it back
        Some(unsafe { std::mem::transmute_copy::<usize, T>(&elem) })
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

    /// Reserve enough space for the provided number of elements
    pub fn reserve(&self, size: usize) {
        // Calculate the number of buckets needed and their indices
        // For each bucket, call allocate_bucket to reserve memory

        // Number of allocations needed for current size
        let mut num_current_allocs = highest_bit(self.size() + FIRST_BUCKET_SIZE - 1)
            .saturating_sub(highest_bit(FIRST_BUCKET_SIZE));
        // Compare num_current_allocs to number of allocations needed for size `new`
        println!("{num_current_allocs}");
        while num_current_allocs
            < highest_bit(size + FIRST_BUCKET_SIZE - 1)
                .saturating_sub(highest_bit(FIRST_BUCKET_SIZE))
        {
            num_current_allocs += 1;
            println!("Allocating bucket: {num_current_allocs}");
            self.allocate_bucket(num_current_allocs as usize);
        }
    }

    pub fn size(&self) -> usize {
        /*
        1. Pull down the current descriptor
        2. Read the size from the descriptor
        3. If there is a pending writeop, subtract one from the size
        4. Return the size
        */
        // SAFETY
        // We know that the raw pointer is pointing to a valid descriptor
        // Because the vector started with a valid instance and the only
        // changes to vector.descriptor are through compare-and-swap
        let desc = unsafe { &*self.descriptor.load(Ordering::Acquire) };
        let size = desc.size;
        // SAFETY
        // We know that the raw pointer is pointing to a valid writedescriptor
        // Because the vector started with a valid writedescriptor
        // and changes can only be made through compare-and-swap
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
        let ptr = allocator.allocate(layout).expect("Out of memory").as_ptr() as *mut AtomicUsize;
        // If the CAS fails, then the bucket has already been initalized with memory
        // and we free the memory we just allocated
        if self.buffers[bucket]
            .compare_exchange(
                std::ptr::null_mut::<AtomicUsize>(),
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

impl<'a, T> Default for SecVec<'a, T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod test {
    use super::SecVec;

    #[test]
    fn new_does_not_sigsegv() {
        let _sv = SecVec::<isize>::new();
    }

    #[test]
    fn one_push_one_pop() {
        let sv = SecVec::<isize>::new();
        sv.push(-69);
        assert_eq!(sv.pop(), Some(-69))
    }
}
