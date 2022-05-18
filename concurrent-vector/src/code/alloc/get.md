# `get()`

The first, and simplest function to write is `vector.get(i)`, which returns a
pointer to the element at index _i_.

This is the code I wrote:

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

Noticed how I've marked the function as `unsafe`. This is because there is a
safety contract the compiler can't enforce: the index must be valid. This is
automatically guaranteed through the usage of the function in the algorithm, but
I marked it `unsafe` just to be explicit.

The function is pretty straightforward: we calculate which buffer the item is
in, load the pointer to the start of the buffer, and offset it to the correct
element. There are two things I want to point out. First, notice all the checks
we make to avoid overflow. Secondly, notice the use of `Ordering::Acquire` for
loading in the pointer to the buffer. Since we always store the pointer with
`Ordering::Release`, we are guaranteed to get the most recent pointer, because
an `Acquire` load cannot get ordered before a `Release` store! I find it very
satisfying how `Acquire` and `Release` work together. It's like two puzzle
pieces fitting nicely into each other

## What are all these bitwise operations?

TODO

Next up is the `allocate_bucket`.

