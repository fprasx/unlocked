# `complete_write()`

I think `complete_write` is the first function I wrote for the vector's core
operations. We execute the `WriteDescriptor` passed in and set the one stored in
the vector to `None`

Here's what the function signature looks like:

```rust
fn complete_write(&self, pending: &Option<WriteDescriptor<T>>) {

```

The first thing we do is execute the `WriteDescriptor`, if there is one. We can
use `if let` syntax to concisely express this. The result of the
`compare_exchange` doesn't matter. If it succeeds, we performed the write. If it
doesn't, someone else performed it. Also, notice how we are
`compare_exchange`ing an `AtomicU64`. The data is transmuted into those bytes,
allowing us to make atomic modifications to the contents of the vector. Because
the data needs to be transmuted into an atomic type, the vector can't support
types larger than 8 bytes. Finally, because we are using `AcqRel` as the success
ordering, any subsequent `Acquire` loads will see the that there is no pending
write.

```rust
    #[allow(unused_must_use)]
    if let Some(writedesc) = pending {
        AtomicU64::compare_exchange(
            writedesc.location,
            writedesc.old,
            writedesc.new,
            Ordering::AcqRel,
            Ordering::Relaxed,
        );

```

Now that we've done the store to the contents, we change the `WriteDescriptor`
status of the vector to indicate that there is no pending write (we just took
care of it!).

```rust
        let new_writedesc = WriteDescriptor::<T>::new_none_as_ptr();
        // # Safety
        // The pointer is valid to dereference because it started off valid
        // and only pointers made from WriteDescriptor::new_*_as_ptr()
        // (which are valid because of Box) are CAS'd in
        unsafe { &*self.descriptor.load(Ordering::Acquire) } // Loading with `Acquire`
            .pending
            .store(new_writedesc, Ordering::Release); // Storing with `Release`

        // Memory leak alert!
        // What happens to the old pointer stored in the
        // `AtomicPtr<Option<WriteDescriptor<T>>>`?
        // We never reclaim it.
    }
}

```

This is standard. We make a new `WriteDescriptor` and store it with `Release` so
that all subsequent `Acquire` loads will see it.

## Leaking memory

Leaking memory is when you use memory (allocating) but never free it. This is
the first chunk of code that leaks. Our `Descriptor` has a pointer to an
`Option<WriteDescriptor>`. When we store a different pointer, we lose the old
pointer forever. Since we never do anything to deallocate the memory pointed to
by the old pointer, like `Box::from_raw`, that memory will stay allocated until
the end of the program.

We can't just directly free the memory right away though, as there could be
another thread reading it. Later on, I'm going to show you how we can use a
technique called _hazard pointers_ to safely reclaim (deallocate) objects.

For now, the vector will stay leaky, and we'll move on the `push`.

---

### Complete source for `complete_write()`

```rust
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
        // The pointer is valid to dereference because it started off valid
        // and only pointers made from WriteDescriptor::new_*_as_ptr()
        // (which are valid because of Box) are CAS'd in
        unsafe { &*self.descriptor.load(Ordering::Acquire) }
            .pending
            .store(new_writedesc, Ordering::Release);
    }
}

```
