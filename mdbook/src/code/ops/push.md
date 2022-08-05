# `push()`

You made it! We're going to implement half of the main functionality of the
vector. The code is going to get a little complex, but I'm confident in you. I
eventually understood what was going on, so you can too.

We're going to track the steps described in
[**The Algorithm**](../../paper/algorithm.md) closely. We don't want to mess up
the concurrent semantics of the vector during implementation. The first thing we
do is load in the `Descriptor` and `WriteDescriptor`. This is actually harder
than it might seem, as we're working with `unsafe` things like raw pointers. We
need to be very careful. But wait, there's one more thing we should cover, and
that's _exponential backoff_!

## Exponential Backoff

Exponential backoff is another one of those techniques that's unique to
concurrent programming. `compare_exchange` algorithms like the one we're
implementing can produce a lot of contention over a couple specific memory
locations. For example, may threads are trying to `compare_exchange` the
`AtomicPtr<Descriptor>` stored in the vector. That spot in memory is constantly
bombarded with heavy atomic operations. One way we can alleviate this is by
waiting a little bit after failing to `compare_exchange`. The first time we
fail, we back off for `1` tick. If we fail again, we back off for `2` ticks,
then `4`, `8` . . . this is why the _backoff_ is _exponential_. By backing off,
we give another thread some room to successfully perform their
`compare_exchange`. In some mircobenchmarks I did, introducing exponential
backoff greatly speeded up the vector. It's cool that going slower at a micro
level allows us to go faster on a macro level. `crossbeam_utils` has a useful
little `struct` called `Backoff` that we're going to use.

```rust
pub fn push(&self, elem: T) {
    let backoff = Backoff::new(); // Backoff causes significant speedup
    loop {
        // # Safety
        // It is safe to dereference the raw pointers because they started off valid
        // and can only be CAS'd with pointers from `Box::into_raw`
        let current_desc = unsafe { &*self.descriptor.load(Ordering::Acquire) };

        // Complete a pending write op if there is any
        let pending = unsafe { &*current_desc.pending.load(Ordering::Acquire) };

```

There is already a lot going on here, in just these 10ish lines of code.
Firstly, we've instantiated a `Backoff`. A the bottom of the loop, if we failed
to `compare_exchange` in our new `Descriptor`, we'll call `Backoff::spin()` to
wait a little bit, then we'll come back up to the top of the loop and try again.

This code also contains a very `unsafe` operation: dereferencing a raw pointer.
The more I read about the dangers of raw pointers, the more scared I got.
Paraphrasing from
[The Book](https://doc.rust-lang.org/book/ch19-01-unsafe-rust.html?highlight=raw%20pointer#dereferencing-a-raw-pointer),
raw pointers aren't guaranteed to point to valid memory, aren't guaranteed to be
non-null, don't implement cleanup (like `Box`), and ignore all the aliasing
rules (`&/&mut` semantics).

After watching
[Demystifying `unsafe` code](https://www.youtube.com/watch?v=QAz-maaH0KM) I felt
better. `unsafe` code isn't intrinsically bad, it's just code that comes with an
extra contract that we must uphold and document.

In the case of these first raw pointer dereferences, we know the dereference is
safe because the pointers to the `Descriptor` and `WriteDescriptor` come from
`Box::into_raw`, which returns a non-null and aligned pointer. `unsafe` is
scary, but not necessarily bad. Obviously, we should try to limit its uses as
much as possible though, as we can slip up and violate contracts.

> Mitigating `unsafe` code: there are ways we can construct API's that need
> `unsafe` code to work without exposing users to danger. For example, we could
> make a type `AtomicBox<T>` that's mostly a wrapper around `AtomicPtr<T>`. It
> might look a little something like this:
>
> ```rust
> #[repr(transparent)]
> struct AtomicBox<T> {
>     ptr: AtomicPtr<T>
> }
>
> impl<T> AtomicBox<T> {
>     // We can only make a `Self` from a `Box`'s pointer!
>     pub fn new(box: Box<T>) -> Self {
>         AtomicPtr::new(Box::into_raw(box))
>     }
>
>     // Caller knows they are receiving a pointer from `Box`
>     pub fn load(&self, ordering: Ordering) -> *mut T {
>         self.0.load(ordering)
>     }
>
>     // -- snip --
> }
>
> ```
>
> There's nothing super crazy going on here, it's just that we've configured the
> API so that we **know** the pointer inside the `AtomicBox<T>` is valid because
> it could only have come from `Box`. Now, instead of manually ensuring the
> invariant that we use `Box::into_raw` pointers, the compiler/type system does
> so for us.

After loading in the `WriteDescriptor`, we execute it if need be.

```rust
    self.complete_write(pending);

```

Since we're `push`ing onto the vector, we might need more memory:

```rust
    // Calculate which bucket this element is going into
    let bucket = (highest_bit(current_desc.size + FIRST_BUCKET_SIZE)
        - highest_bit(FIRST_BUCKET_SIZE)) as usize;

    // If the bucket is null, allocate the memory
    if self.buffers[bucket].load(Ordering::Acquire).is_null() {
        self.allocate_bucket(bucket)
    }

```

Let's make our new `WriteDescriptor` now:

```rust
    // # Safety
    // It is safe to call `self.get()` because if the vector has reached
    // `current_desc.size`, so there is a bucket allocated for element `size`.
    // Therefore, the pointer is also valid to dereference because it points
    // into properly allocated memory.
    let last_elem = unsafe { &*self.get(current_desc.size) };
    let write_desc = WriteDescriptor::<T>::new_some_as_ptr(
        unsafe { mem::transmute_copy::<T, u64>(&elem) },
        // Load from the AtomicU64, which really contains the bytes for T
        last_elem.load(Ordering::Acquire),
        last_elem,
    );

```

For now we are assuming that the vector is only storing values 8 bytes big,
therefore it is safe to `transmute_copy` to an `AtomicU64`. I plan on writing a
macro that produces different implementations of the vector with different
atomic types when storing types of different sizes. For example,
`SecVec<(i8, i8)>` would store the data in `AtomicU16`. This would save on
space. I don't think the vector would work for zero-sized types because of how
we `transmute`. It would also be very inefficient because of all the unnecessary
allocations!

Note that `last_elem`'s type is `&AtomicU64`; it's the location of the write.
When we load from `last_elem`, we are getting the `old` element. We now have the
three pieces of data necessary for `compare_exchange`: a memory location (the
reference), an old element, and a new element (the `T` passed to this function).

Let's package everything up in a `Descriptor`.

```rust
    let next_desc = Descriptor::<T>::new_as_ptr(write_desc, current_desc.size + 1);

```

Since we are adding one more element onto the vector, the new `Descriptor`'s
size is one more than the old one's.

Here comes the crucial `compare_exchange`, in the `AcqRel/Relaxed` flavor:

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
        // We know the current write_desc is the one we just sent in
        // with the compare_exchange so avoid loading it atomically
        self.complete_write(unsafe { &*write_desc });
        break;
    }

```

If the `compare_exchange` succeeds, we call `complete_write` on the descriptor
we just made to finalize the changes, then we `break` out of the loop.

If `compare_exchange` fails, we'll simply start over again.

Either way, **we have a memory leak**. If the `compare_exchange` succeeded, we
never deal with the old `Descriptor`'s pointer. We can never safely deallocate
it because we don't know if anyone is reading it. It would be terribly rude to
pull the rug out from under them! Also the deallocation would probably cause a
`use-after-free` which would cause the OS to terminate the program which would
rip a hole in the space-time continuum which would. Wait what? Uhh, moving on .
. .

If the `compare_exchange` failed, the new `Descriptor` and `WriteDescriptor`
leak. Once we reach the end of the loop, all local variables in that scope are
lost. So, we never get back the pointers to our new describe-y objects, and
their memory is lost the the void, never to be seen again (unless we do some
wildly dumb stuff and read a random address or something). In any case, within
the code for the vector, I try not to tempt the segfault gods. My other
projects, maybe a little bit.

At this point, we've failed the `compare_exchange`. Let's `Backoff::spin()` and
then retry:

```rust
        backoff.spin();
    } // Closing brace for the loop
} // Closing brace for the function

```

Once we finish looping and finally succeed with the `compare_exchange`, we're
done! That's a `push`. The pseudocode is so simple, and the code is so . . . not
simple. Props to you for getting this far, concurrent programming is not for the
weak of spirit.

I'll cover the minor differences in `pop`, and then we'll cap off the leaky code
with `size`.

---

### Complete source for `push`

```rust
pub fn push(&self, elem: T) {
    let backoff = Backoff::new(); // Backoff causes significant speedup
    loop {
        // # Safety
        // It is safe to dereference the raw pointers because they started off valid
        // and can only be CAS'd with pointers from `Box::into_raw`
        let current_desc = unsafe { &*self.descriptor.load(Ordering::Acquire) };
        let pending = unsafe { &*current_desc.pending.load(Ordering::Acquire) };

        // Complete a pending write op if there is any
        self.complete_write(pending);

        // Allocate memory if need be
        let bucket = (highest_bit(current_desc.size + FIRST_BUCKET_SIZE)
            - highest_bit(FIRST_BUCKET_SIZE)) as usize;
        if self.buffers[bucket].load(Ordering::Acquire).is_null() {
            self.allocate_bucket(bucket)
        }
        // # Safety
        // It is safe to call `self.get()` because if the vector has reached
        // `current_desc.size`, so there is a bucket allocated for element `size`.
        // Therefore, the pointer is also valid to dereference because it points
        // into properly allocated memory.
        let last_elem = unsafe { &*self.get(current_desc.size) };

        let write_desc = WriteDescriptor::<T>::new_some_as_ptr(
            unsafe { mem::transmute_copy::<T, u64>(&elem) },
            last_elem.load(Ordering::Acquire),
            last_elem,
        );

        let next_desc = Descriptor::<T>::new_as_ptr(write_desc, current_desc.size + 1);

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
            // We know the current write_desc is the one we just sent in
            // with the compare_exchange so avoid loading it atomically
            self.complete_write(unsafe { &*write_desc });
            break;
        }

        backoff.spin();
    }
}

```
