# Dropping the vector

We approach the end! As its last action, the vector will free the memory
allocated in its buckets and the `Descriptor` it holds. Once again, we achieve
this by implementing the `Drop` trait.

Using a bunch of chained function calls on `&self.buffers`, we can get all of
the buffers that aren't null. Then, we recreate the `Layout` they hold and
deallocate them.

Dropping the current `Descriptor` is simple, we just `Box::from_raw` it!. It's
destructor runs and the inner `WriteDescriptor` is also deallocated.

```rust
impl<T> Drop for SecVec<'_, T>
where
    T: Copy,
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
                // We have recreated the exact same layout used to alloc the ptr in
                // `allocate_bucket`. We know the ptr isn't null because of the filter
                allocator.deallocate(
                    NonNull::new(ptr.load(Ordering::Relaxed) as *mut u8).unwrap(),
                    layout,
                )
            };
        }

        // Retiring the current desc and wdesc
        // # Safety
        // Since we have &mut self, we have exclusive access, so we can retire the
        // desc and wdesc ptrs.
        //
        // It is safe to dereference the ptr to the desc because it is valid because
        // it was created with Descriptor::new_as_ptr.
        let desc = self.descriptor.load_ptr();
        unsafe {
            Box::from_raw(desc);
        };
    }
}

```

That's it. All the leaks. All the code I'm going to show you. I hope that was a
satisfying journey from learning about the algorithm to fully implementing it!.
Let's run the tests one more time with `Miri :)`
