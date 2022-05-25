# `allocate_bucket()`

Remember that whole "two-level array" thingy? This is where it starts coming
into play. `allocate_bucket` does just what is sounds like: allocating a bucket.
Recall that a bucket is one of the arrays in the _second_ level of the two level
array.

```
+---+---+---+---+---+
| 1 | 2 | 3 | 4 | 5 |
+---+---+---+---+---+
  |   |   |   |   |
  v   v   v   v   v
+---+---+---+---+---+
| 1 | 2 | 3 | 4 | 5 |
+---+---+---+---+---+
    | 2 | 3 | 4 | 5 |
    +---+---+---+---+
        | 3 | 4 | 5 |
        +---+---+---+
          ^ | 4 | 5 |
          | +---+---+
          |     | 5 |
          |     +---+
          |
        we're allocating one of these little guys
```

There are two parts to `allocate_bucket`: allocating the memory and setting the
pointer. We start off by tapping into the `alloc` crate's API. First, we create
a `Layout`, which describes the allocation we want. The
`Layout::array::<Atomic64>()` indicates that we want a bunch of `AtomicU64`
right next to each other in memory. If creating the layout fails (due to
overflow), we call `capacity_overflow`, which just panics.

> You might ask, why not just directly call `panic!`? Apparently, it reduces the
> generated code size if we just have panic in one function, which we then call
> from multiple places. I found this trick in the source code for
> [`std::vec::Vec`](https://github.com/rust-lang/rust/blob/master/library/alloc/src/raw_vec.rs#L512-L518).
> You can learn a _lot_ from reading the Standard Library code. That's how I've
> learned a lot of the low level stuff I know. It's also a good way to see what
> good, idiomatic Rust looks like.

```rust
const FIRST_BUCKET_SIZE: usize = 8;

fn allocate_bucket(&self, bucket: usize) {
    // The shift-left is equivalent to raising 2 to the power of bucket
    let size = FIRST_BUCKET_SIZE * (1 << bucket);
    let layout = match Layout::array::<AtomicU64>(size) {
        Ok(layout) => layout,
        Err(_) => capacity_overflow(),
    };

```

The next thing we do is just another check. The Standard Library does both checks and I
trust their
[strategy](https://github.com/rust-lang/rust/blob/master/library/alloc/src/raw_vec.rs#L176-L183).

```rust
// Make sure allocation is ok
match alloc_guard(layout.size()) {
    Ok(_) => {}
    Err(_) => capacity_overflow(),
}

```

> [`Miri`](https://github.com/rust-lang/miri) is about to make its debut! `Miri`
> is a tool that runs your code in a special environment and detects undefined
> behavior (or UB, as the cool kids call it).

Now that our layout is all good, we can perform the actual allocation. We
instantiate the `Global` `struct`, which is the allocator we're using. The
allocator returns a pointer to our new allocation once it's finished allocating.
Why are we using `allocated_zeroed` you might ask? Why not just allocate
normally? The answer: _It's Utmost Holiness:_ `Miri`. In all seriousness though,
`Miri` has been and invaluable tool in catching memory and concurrency bugs.
When we just allocate normally, `Miri` throws and error when we start actually
using the memory later on, saying that
`intrinsics::atomic_cxchg_acqrel_failrelaxed(dst, old, new)` requires
initialized data. Thus, we just zero the memory for now. Later, it might be
worth it to do some `MaybeUninit` magic, but honestly, I don't know if there'll
be much, if any, performance gains.

Once again, we have more checks, and we'll just `panic!` if the allocation
fails. `handle_alloc_error` is from the `alloc` crate:

```rust
let allocator = Global;

let allocation = allocator.allocate_zeroed(layout);
let ptr = match allocation {
    Ok(ptr) => ptr.as_ptr() as *mut AtomicU64,
    Err(_) => handle_alloc_error(layout),
};

```

The final part is to swap in the pointer into our array of buffers (the first
level of the two-level array). We use the `compare_exchange` function, with a
null pointer as the expected value, and our new pointer from the allocation. If
`compare_exchange` fails, that means the pointer is no longer null, and someone
else `compare_exchanged`ed in a pointer. Therefore, the bucket is already
allocated. In this case, we deallocate the freshly allocated memory. Notice how
we assess the result of `compare_exchange` with `Result::is_err()`; we don't
care about the value `compare_exchange` returns.

```rust
    if self.buffers[bucket] // <- this is an AtomicPtr<AtomicU64>
        .compare_exchange(
            ptr::null_mut::<AtomicU64>(), // old value
            ptr, // new value
            Ordering::AcqRel, // ordering on success
            Ordering::Relaxed, // ordering on fail
        )
        .is_err()
    {
        unsafe {
            // # Safety
            // We know that the pointer returned from the allocation is NonNull so
            // we can call unwrap() on NonNull::new(). We also know that the pointer
            // is pointing to the correct memory because we just got it from the
            // allocation. We know the layout is valid, as it is the same layout we
            // used to allocate.
            allocator.deallocate(NonNull::new(ptr as *mut u8).unwrap(), layout);
        }
    }
}

```

## Success and Fail Orderings

Like all atomic operations, `compare_exchange` uses the orderings. Most
operations take 1, but this bad boy takes two. Since `compare_exchange` reads
and writes a memory location, we're using `AcqRel`. Since we always use
`AcqRel` for the buckets, the load part (`Acquire`) of the `compare_exchange`
will always see the most recent value because the store part is `Release`. If we
just used `Acquire`, the store part of the `compare_exchange` would be
`Relaxed`, which doesn't guarantee that the modification to the
`AtomicPtr<AtomicU64>` is published to other threads by any certain point. Under
a `Relaxed` situation, another thread might load a null pointer in its
`compare_exchange`, even though our thread swapped in a pointer to memory!

That's the success ordering. The fail ordering is `Relaxed` because we don't
need to establish any synchronization if the operation fails. It failed; we're
not doing any stores. When I first saw this, I had the question, "Why do we
provide different success and fail orderings if the `compare_exchange` doesn't
know if it will fail or not?" The answer, thanks to Alice on the Rust User
Forums, is that the compiler picks an ordering that will always satisfy the stronger
ordering. Thus, `compare_exchange(success: AcqRel, fail: Release)` executes as
`compare_exchange(success: AcqRel, fail: Acquire)` to ensure that the initial
load is `Acquire` for both cases.

There's a little more to it; if you're still curious, see this
[thread](https://users.rust-lang.org/t/what-does-the-compare-exchange-fail-ordering-mean/75791)
on the Rust User Forums.

The last function in the "memory" section is `reserve()`, which I've "reserved" for last.

---

### Complete source for `allocate_bucket()`

```rust
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
    // allocate_zeroed because miri complains about accessing uninitialized memory
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
            // # Safety
            // We know that the pointer returned from the allocation is NonNull so
            // we can call unwrap() on NonNull::new(). We also know that the pointer
            // is pointing to the correct memory because we just got it from the
            // allocation. We know the layout is valid, as it is the same layout we
            // used to allocate.
            allocator.deallocate(NonNull::new(ptr as *mut u8).unwrap(), layout);
        }
    }
}

```
