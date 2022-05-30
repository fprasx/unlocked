# Fixing push & pop

I know I said that the changes to `push` and `pop` aren't that bad, which is
true. Getting to those changes however, took a while. I'm going to explain what
I did with pseudocode first, and then show the final code.

The first thing I tried was just retiring the old `Descriptor` after a
successful `compare_exchange`, however, this didn't reduce the leakage at all
for some reason. I figured it might be because the `Descriptor` was pointing a
live `WriteDescriptor`. So then, I also retired the `WriteDescriptor`. However,
this produced use-after-frees and data races according to `Miri`, so I knew I
was doing something wrong.

I decided to review the safety contract of `retire_in` again, and that is when I
found the bug. Retiring the `Descriptor` is safe for the same reason retiring
the `WriteDescriptor` after `complete_write` is. Since the `Descriptor` is the
result of a swap, we are the only thread who will retire it. The thing is, if we
also retire the `WriteDescriptor`, a thread who is already accessing the
`Descriptor` could make a _new_ load to the just retired `WriteDescriptor`,
violating the safety contract of `retire_in`, and causing UB.

## The problem in picture form

We, Thread 1, have the `Descriptor` as the result of a successful
`compare_exchange`. Thread 2 is also reading the `Descriptor` (**but not the
inner `WriteDescriptor`**)

```
               Thread 2
               /
Thread 1 (us) /
   |         /
   |        /
   V       v
  Descriptor
     \
      \
       \
        v
        WriteDescriptor
```

Because the `compare_exchange` was successful, we `retire` the `Descriptor` and
`WriteDescriptor`. The `Descriptor` is protected from reclamation because Thread
2 is reading it, but the `WriteDescriptor` has no readers so it gets
deallocated.

```
               Thread 2
               /
Thread 1 (us) /
   |         /
   |        /
   V       v
  Descriptor
     \
   ---+----------------
       \
        v
        WriteDescriptor <Deallocated>
```

Now, Thread 2 goes to read the (now reclaimed!!) `WriteDescriptor` by loading
the pointer contained in the `Descriptor` (which is still protected, and safe to
access).

```
               Thread 2
                  |
Thread 1 (us)     |
   |              |
   |              |
   V              |
  Descriptor      |
     \            |
   ---+-----------+----
       \          |
        v         V
        WriteDescriptor <Deallocated>
```

And here we have it, Thread 2 accessing deallocated memory!

## The solution

The solution I came up with is to make sure a reference to a `WriteDescriptor`
never outlives the reference to it's parent `Descriptor`. Visually this looks
like:

```
-- Descriptor Reference Start

    -- WriteDescriptor Reference Start


    -- WriteDescriptor Reference End



-- Descriptor Reference End
```

This means that when there are no people accessing a `Descriptor`, there are
also no people accessing the inner `WriteDescriptor`. Therefore, when a
`Descriptor` is `retired`ed, the `WriteDescriptor` is also safe to `retire`
because there are no references to it. Since no one can get a new reference to a
`retire`ed `Descriptor`, no once can access the inner `WriteDescriptor`.

Why is this important? Whenever we reclaim a `Descriptor`, we also reclaim the
inner `WriteDescriptor`, fixing our leaks without causing any UB.

To implement this custom behavior for `Descriptor`, we implement the `Drop`
trait. A type that implements `Drop` executes some custom behavior when it goes
out of scope and is reclaimed.

The `Drop` implementation looks like this:

```rust
impl<T> Drop for Descriptor<'_, T>
{
    fn drop(&mut self) {
        // # Safety
        // The pointer is valid because it's from Box::into_raw
        // We must also ensure ref to wdesc never outlasts ref to desc
        unsafe {
            Box::from_raw(
                self.pending
                    .swap_ptr(ptr::null_mut())
                    .unwrap()
                    .into_inner() // This is a NonNull<T>
                    .as_ptr() // Turn it into a raw pointer
            );
        }
    }
}
```

All we're doing is extracting the pointer to the `WriteDescriptor` and calling
`Box::from_raw` on it so that its memory will be reclaimed by `Box` when it
goes out of scope.

## Reclaiming the `Descriptor`s

Its time to finally go over the code changes to `push`. All accesses to the
`Descriptor` and `WriteDescriptor` are guarded with a hazard pointer. The access
returns a reference to the `Descriptor`/`WriteDescriptor`, which is valid as
long as the hazard pointer guarding the access is alive. Access to the inner
`WriteDescriptor` is explicitly scoped within its own block to make clear that
access to the `WriteDescriptor` cannot outlive the access to the parent
`Descriptor`.

```rust
pub fn push(&self, elem: T) {
    let backoff = Backoff::new(); // Backoff causes significant speedup
    loop {
        let mut dhp = HazardPointer::new_in_domain(&self.domain);
        let current_desc = unsafe { self.descriptor.load(&mut dhp) }
            .expect("invalid ptr for descriptor in push");

        // Use a block to make explicit that the use of the wdesc does not outlive
        // the use of the desc.
        // This means that when the desc is dropped, there will be no references
        // to the wdesc inside.
        // And we can deallocate the wdesc with `Box::from_raw`
        {
            let mut wdhp = HazardPointer::new_in_domain(&self.domain);
            let pending = unsafe { current_desc.pending.load(&mut wdhp) }
                .expect("invalid ptr from write-desc in push");

            self.complete_write(pending as *const _ as *mut _);
            // Hazard pointer is dropped, protection ends
        }
```

This stuff is all the same as before.

```rust
        // If we need more memory, calculate the bucket
        let bucket = (highest_bit(current_desc.size + FIRST_BUCKET_SIZE)
            - highest_bit(FIRST_BUCKET_SIZE)) as usize;
        // Allocate it
        if self.buffers[bucket].load(Ordering::Acquire).is_null() {
            self.allocate_bucket(bucket)
        }

        let last_elem = unsafe { &*self.get(current_desc.size) };

        let next_write_desc = WriteDescriptor::<T>::new_some_as_ptr(
            // TODO: address this in macro
            // # Safety
            // The `transmute_copy` is safe because we have ensured that T is the
            // correct size at compile time
            unsafe { mem::transmute_copy::<T, u64>(&elem) },
            // Load from the AtomicU64, which really contains the bytes for T
            last_elem.load(Ordering::Acquire),
            last_elem,
        );

        let next_desc = Descriptor::<T>::new_as_ptr(next_write_desc,
            current_desc.size + 1);

```

The `compare_exchange` syntax is slightly different, but it's doing the exact
same thing. We don't have to specify orderings because they're built in by
`haphazard`. On a successful `compare_exchange`, we `retire` the pointer
to the old `Descriptor`. When it is finally reclaimed, its `Drop`
implementation will run and its inner `WriteDescriptor` will also get reclaimed
safely.

If the `compare_exchange` fails, we deallocate our local `Descriptor` normally
by calling `Box::from_raw`. Since the local `Descriptor` was never shared across
threads, we don't have to worry about synchronizing the deallocation. Then, we
spin using the `Backoff` and go back to the top of the loop.

```rust
        if let Ok(replaced) = unsafe {
            HazAtomicPtr::compare_exchange_weak_ptr(
                // # Safety
                // Safe because the pointer we swap in points to a valid object that
                // is !null
                &self.descriptor,
                current_desc as *const _ as *mut _,
                next_desc,
            )
        } {
            self.complete_write(next_write_desc);

            // # Safety
            // Since the we only retire when swapping out a pointer, this is the only
            // thread that will retire, since only one thread receives the result of
            // the swap (this one)
            //
            // There will never be another load call to the ptr because all calls will
            // go the new one. Since all uses of the inner wdesc are contained within
            // the lifetime of the reference to the desc, there will also be no new
            // loads on the inner wdesc.
            unsafe {
                replaced.unwrap().retire_in(&self.domain);
            }
            break;
        }

        // Deallocate the write_desc and desc that we failed to swap in
        // # Safety
        // Box the write_desc and desc ptrs were made from Box::into_raw, so it
        // is safe to Box::from_raw
        unsafe {
            // Note: the inner wdesc also get's dropped as part of the desc's drop impl
            Box::from_raw(next_desc);
        }

        backoff.spin();
    }
}

```

The changes for `pop` are identical. We are so close to being done with code.
Our `Descriptor`s and `WriteDescriptors` are eventually reclaimed, which is a
big step forward. The last thing is to deallocate the buckets and the final
`Descriptor` when the vector itself is dropped.

---

### Complete source for `push()` and `pop()`

```rust
pub fn push(&self, elem: T) {
    let backoff = Backoff::new(); // Backoff causes significant speedup
    loop {
        let mut dhp = HazardPointer::new_in_domain(&self.domain);
        let current_desc = unsafe { self.descriptor.load(&mut dhp) }
            .expect("invalid ptr for descriptor in push");

        // Use a block to make explicit that the use of the wdesc does not
        // outlive the use of the desc. This means that when the desc is dropped,
        // there will be no references to the wdesc inside.And we can deallocate
        // the wdesc with `Box::from_raw`
        {
            let mut wdhp = HazardPointer::new_in_domain(&self.domain);
            let pending = unsafe { current_desc.pending.load(&mut wdhp) }
                .expect("invalid ptr from write-desc in push");

            self.complete_write(pending as *const _ as *mut _);
            // Hazard pointer is dropped, protection ends
        }

        // If we need more memory, calculate the bucket
        let bucket = (highest_bit(current_desc.size + FIRST_BUCKET_SIZE)
            - highest_bit(FIRST_BUCKET_SIZE)) as usize;
        // Allocate it
        if self.buffers[bucket].load(Ordering::Acquire).is_null() {
            self.allocate_bucket(bucket)
        }

        let last_elem = unsafe { &*self.get(current_desc.size) };

        let next_write_desc = WriteDescriptor::<T>::new_some_as_ptr(
            // TODO: address this in macro
            // # Safety
            // The `transmute_copy` is safe because we have ensured that T is
            // the correct size at compile time
            unsafe { mem::transmute_copy::<T, u64>(&elem) },
            // Load from the AtomicU64, which really contains the bytes for T
            last_elem.load(Ordering::Acquire),
            last_elem,
        );

        let next_desc = Descriptor::<T>::new_as_ptr(next_write_desc,
            current_desc.size + 1);

        if let Ok(replaced) = unsafe {
            HazAtomicPtr::compare_exchange_weak_ptr(
                // # Safety
                // Safe because the pointer we swap in points to a valid object that
                // is !null
                &self.descriptor,
                current_desc as *const _ as *mut _,
                next_desc,
            )
        } {
            self.complete_write(next_write_desc);

            // # Safety
            // Since the we only retire when swapping out a pointer, this is the only
            // thread that will retire, since only one thread receives the result of
            // the swap (this one)
            //
            // There will never be another load call to the ptr because all calls will
            // go the new one. Since all uses of the inner wdesc are contained within
            // the lifetime of the reference to the desc, there will also be no new
            // loads on the inner wdesc.
            unsafe {
                replaced.unwrap().retire_in(&self.domain);
            }
            break;
        }

        // Deallocate the write_desc and desc that we failed to swap in
        // # Safety
        // Box the write_desc and desc ptrs were made from Box::into_raw, so it is
        // safe to Box::from_raw
        unsafe {
            // Note: the inner wdesc also get's dropped as part of the desc's drop impl
            Box::from_raw(next_desc);
        }

        backoff.spin();
    }
}

pub fn pop(&self) -> Option<T> {
    let backoff = Backoff::new(); // Backoff causes significant speedup
    loop {
        let mut dhp = HazardPointer::new_in_domain(&self.domain);
        let current_desc = unsafe { self.descriptor.load(&mut dhp) }
            .expect("invalid ptr for descriptor in pop");

        // Use a block to make explicit that the use of the wdesc does not
        // outlive the use of the desc. This means that when the desc is
        //  dropped, there will be no references to the wdesc inside.
        // And we can deallocate the wdesc with `Box::from_raw`
        {
            let mut wdhp = HazardPointer::new_in_domain(&self.domain);
            let pending = unsafe { current_desc.pending.load(&mut wdhp) }
                .expect("invalid ptr for write-descriptor in pop");

            self.complete_write(pending as *const _ as *mut _);
            // Hazard pointer is dropped, protection ends
        }

        if current_desc.size == 0 {
            return None;
        }

        // TODO: add safety comment
        // Consider if new desc is swapped in, can we read deallocated memory?
        // Do not need to worry about underflow for the sub because we would
        // have already returned
        let elem = unsafe { &*self.get(current_desc.size - 1) }
            .load(Ordering::Acquire);

        let new_pending = WriteDescriptor::<T>::new_none_as_ptr();

        let next_desc = Descriptor::<T>::new_as_ptr(new_pending,
            current_desc.size - 1);

        if let Ok(replaced) = unsafe {
            HazAtomicPtr::compare_exchange_weak_ptr(
                // # Safety
                // Safe because the pointer we swap in points to a valid object that
                // is !null
                &self.descriptor,
                current_desc as *const _ as *mut _,
                next_desc,
            )
        } {
            // # Safety
            // Since the we only retire when swapping out a pointer, this is the only
            // thread that will retire, since only one thread receives the result of
            // the swap (this one)
            //
            // There will never be another load call to the ptr because all calls will
            // go the new one. Since all uses of the inner wdesc are contained within
            // the lifetime of the reference to the desc, there will also be no new
            // loads  on the inner wdesc.
            unsafe {
                replaced.unwrap().retire_in(&self.domain);
            }

            // # Safety
            // TODO: address this in macro
            // This is ok because we ensure T is the correct size at compile time
            // We also know that elem is a valid T because it was transmuted into a
            // usize from a valid T, therefore we are only transmuting it back
            return Some(unsafe { mem::transmute_copy::<u64, T>(&elem) });
        }

        // Deallocate the write_desc and desc that we failed to swap in
        // # Safety
        // Box the write_desc and desc ptrs were made from Box::into_raw, so
        // it is safe to Box::from_raw
        unsafe {
            // Note: the inner wdesc also get's dropped as part of the desc's drop impl
            Box::from_raw(next_desc);
        }

        backoff.spin();
    }
}
```
