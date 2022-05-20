# `push()`

You made it! We're going to implement half of the main functionality of the
vector. The code is going to get a little complex, but I'm confident in you. I
eventually understood what was going on, so you can too.

We're going to track the steps described in
[**The Algorithm**](../../paper/algorithm.md) closely. We don't want to mess up
the concurrent semantics of the vector during implementation. The first thing we
do is load in the `Descriptor` and `WriteDescriptor`. This is actually harder
than it might seem, as we're working with `unsafe` things like raw pointers. We
need to be very careful. But wait, there's one more thing I want to cover, and
that's _exponential backoff_!

## Exponential Backoff

Exponential backoff is another one of those techniques that's unique to
concurrent programming. `compare_exchange` algorithms like the one we're
implementing can produce a lot of contention over a couple specific memory
locations. For example, may threads are trying to `compare_exchange` the
`AtomicPtr<Descriptor<T>>` stored in the vector. That spot in memory is
constantly bombarded with heavy atomic operations. One way we can alleviate this
is by waiting a little bit after failing to `compare_exchange`. The first time
we fail, we back off for `1` tick. If we fail again, we back off for `2` ticks,
then `4`, `8` . . . this is why the _backoff_ is expoenential. In some
mircobenchmarks I did, introduceing exponential backoff greatly speeded up the
vector. `crossbeam_utils` has a useful little `struct` call `Backoff` that we're
going to use. Ok, back to the code.

```rust
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

```

There is already a lot going on here, in just these 10ish lines of code.
Firstly, we've instantiated a `Backoff`. A the end of the loop, if we failed to
`compare_exchange` in our new `Descriptor`, we'll call `Backoff::spin()` to wait
a little bit. Also notice how everything is happening inside the `loop`. The
condition for exiting happens at the bottom.



```
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

```
