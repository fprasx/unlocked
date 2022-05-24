# Fixing complete_write

`complete_write` was the easiest leak to seal. When we swap out the
`WriteDescriptor`, we get back the old one. All we have to do is retire it, and
its memory will eventually get reclaimed. Using the `haphazard` crate did
require some slight changes to code, which I'll point out.

We execute the `WriteDescriptor` and make a new one (`None`) like normal:

```rust
fn complete_write(&self, pending: *mut Option<WriteDescriptor<T>>) {
    // If cas of actual value fails, someone else did the write
    // Result of cmpxchng doesn matter
    if let Some(writedesc) = unsafe { &*pending } {
        let _ = AtomicU64::compare_exchange(
            writedesc.location,
            writedesc.old,
            writedesc.new,
            Ordering::AcqRel,
            Ordering::Relaxed,
        );

        let new_writedesc = WriteDescriptor::<T>::new_none_as_ptr();

```

Here comes the part where the hazard pointers kick in. Firstly, we make a hazard pointer in `&self.domain`

```rust
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

```
