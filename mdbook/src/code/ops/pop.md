# pop

There are three main differences between `pop` and `push`. Firstly, `pop` never
needs to allocate. Secondly, `pop` swaps in a slightly different descriptor,
with `None` as the `WriteDescriptor` and `current_desc.size - 1` as the new
size.

```rust
    let new_pending = WriteDescriptor::<T>::new_none_as_ptr();
    let next_desc = Descriptor::<T>::new_as_ptr(new_pending, current_desc.size - 1);

```

The final difference is that after we succeed with the `compare_exchange`, we
read the last element and return it.

```rust
    if AtomicPtr::compare_exchange_weak(
        &self.descriptor,
        current_desc as *const _ as *mut _,
        next_desc,
        Ordering::AcqRel,
        Ordering::Relaxed,
    )
    .is_ok()
    {
        // # Safety
        // This is ok because only 64-bit values can be stored in the vector
        // We also know that elem is a valid T because it was transmuted into a usize
        // from a valid T, therefore we are only transmuting it back
        return Some(unsafe { mem::transmute_copy::<u64, T>(&elem) });
    }

```

The rest of the function: loading the `Descriptors`, `compare_exchange`,
`Backoff`, is identical.

Like `push`, `pop` also leaks memory profusely. Luckily, this means that when we
implement memory reclamation, it'll be the same solution for `push` and `pop`.

---

### Complete source for `pop()`

```rust
pub fn pop(&self) -> Option<T> {
    let backoff = Backoff::new(); // Backoff causes significant speedup
    loop {
        let current_desc = unsafe { &*self.descriptor.load(Ordering::Acquire) };
        let pending = unsafe { &*current_desc.pending.load(Ordering::Acquire) };

        self.complete_write(pending);
        if current_desc.size == 0 {
            return None;
        }

        // # Safety
        // Do not need to worry about underflow for the sub because we would have
        // already returned
        let elem = unsafe { &*self.get(current_desc.size - 1) }
            .load(Ordering::Acquire);

        let write_desc = WriteDescriptor::<T>::new_none_as_ptr();
        let next_desc = Descriptor::<T>::new_as_ptr(write_desc, current_desc.size - 1);

        if AtomicPtr::compare_exchange_weak(
            &self.descriptor,
            current_desc as *const _ as *mut _,
            next_desc,
            Ordering::AcqRel,
            Ordering::Relaxed,
        )
        .is_ok()
        {
            // # Safety
            // This is ok because only 64-bit values can be stored in the vector
            // We also know that elem is a valid T because it was transmuted into a
            // usize from a valid T, therefore we are only transmuting it back
            return Some(unsafe { mem::transmute_copy::<u64, T>(&elem) });
        }
        backoff.spin();
    }
}

```
