# Dropping the vector

We approach the end! As its last action, the vector will free the mmeory
allocated in its buckets and the `Descriptor` it holds. Once again, we achieve
this by implementing the `Drop` trait.

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

```
