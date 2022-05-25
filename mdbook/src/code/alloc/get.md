# `get()`

The first, and simplest function to write is `vector.get(i)`, which returns a
pointer to the element at index _i_.

Here is the code to that implements `get`.

```rust
/// Return a *const T to the index specified
///
/// # Safety
/// The index this is called on **must** be a valid index, meaning:
/// there must already be a bucket allocated which would hold that index
/// **and** the index must already have been initialized with push/set
unsafe fn get(&self, i: usize) -> *const AtomicU64 {
    // Check for overflow
    let pos = i
        .checked_add(FIRST_BUCKET_SIZE)
        .expect("index too large, integer overflow");

    let hibit = highest_bit(pos);

    let offset = pos ^ (1 << hibit);

    // Select the correct buffer to index into
    // # Safety
    // Since hibit = highest_bit(pos), and pos >= FIRST_BUCKET_SIZE
    // The subtraction hibit - highest_bit(FIRST_BUCKET_SIZE) cannot underflow
    let buffer = &self.buffers[(hibit - highest_bit(FIRST_BUCKET_SIZE)) as usize];

    // Check that the offset doesn't exceed isize::MAX
    assert!(
        offset
            .checked_mul(mem::size_of::<T>())
            .map(|val| val < isize::MAX as usize)
            .is_some(),
        "pointer offset exceed isize::MAX bytes"
    );

    // Offset the pointer to return a pointer to the correct element
    unsafe {
        // # Safety
        // We know that we can offset the pointer because we will have allocated a
        // bucket to store the value. Since we only call values that are
        // `self.descriptor.size` or smaller, we know the offset will not go out of
        // bounds because of the assert.
        buffer.load(Ordering::Acquire).add(offset)
    }
}
```

## A few points to note

Notice how the function is marked as `unsafe`. This is because there is a
safety contract the compiler can't enforce: the index must be valid. This is
automatically guaranteed through the usage of the function in the algorithm, but
it's worth it marking it `unsafe` just to be explicit.

Summarizing what the function does, we calculate which buffer the item is in,
load the pointer to the start of the buffer, and offset it to the correct
element. There are two other things I want to point out. First, notice all the
checks we make to avoid overflow. Secondly, notice the use of `Acquire` for
loading in the pointer to the buffer. Since the store part of the
`compare_exchange(AcqRel)` we use to set the pointer to the buffer is `Release`,
we are guaranteed to get the most recent pointer, because an `Acquire` load sees
the contents `Release`ed by a `Release` store! I find it very satisfying how
`Acquire` and `Release` work together. It's like two puzzle pieces fitting
nicely into each other.

## What are all these bitwise operations?

I'm honestly not sure. That's why they wrote the paper and I didn't `:)`.

Next up is the `allocate_bucket`.
