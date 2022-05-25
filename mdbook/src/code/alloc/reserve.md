# `reserve()`

The goal of `reserve(n)` is simple: allocate enough memory to perform `n` pushes
without allocating more memory.

This is a useful function, because, as we've seen, `allocate_bucket` requires
some heavy atomics with `compare_exchange`. If we can do our allocations in a
calmer scenario with less contention, we'll experience some performance gains.

We start by calculating the number of allocations we'll need to perform to
reserve enough space. The calculation is a little funky, and there's an edge
case where it can't distinguish between 0 and sizes between 1 and
`FIRST_BUCKET_SIZE`. That's why we need to explicitly allocate the first bucket.
We'll see the implementation of `size()` later, but it does use some atomic
synchronization, so we just cache the result so we can keep using it later
without calling `size` again.

```rust
pub fn reserve(&self, size: usize) {
    // Cache the size to prevent another atomic op from due to calling `size()` again
    let current_size = self.size();
    if current_size == 0 {
        self.allocate_bucket(0);
    }

```

Now, we calculate the number of allocations we've made.

`highest_bit` returns the highest set bit in a number. A bit is set
if it's equal to one. The highest set bit of 7 (`0b111`), for example, is 2
(0-indexed). Since the buckets are increasing by a factor of two each time, the
highest set bit of the indices in each bucket is one greater than the highest
set bit of the indices in the previous bucket. Therefore, by using the highest
bit of a number in conjunction with `FIRST_BUCKET_SIZE`, we can figure out how
many allocations are needed for a certain capacity. I know I'm waving my hands a
little; I haven't taken the time to rigorously understand the arithmetic, as
it's not that interesting to me, and in practice it works.

```rust
let mut num_current_allocs =
    highest_bit(current_size.saturating_add(FIRST_BUCKET_SIZE) - 1)
        .saturating_sub(highest_bit(FIRST_BUCKET_SIZE));

```

Then we calculate the number of allocations we need to reserve the space, and
for each allocation missing, we allocate.

```rust
    // Compare with the number of allocations needed for size `new`
    while num_current_allocs
        < highest_bit(size.saturating_add(FIRST_BUCKET_SIZE) - 1)
            .saturating_sub(highest_bit(FIRST_BUCKET_SIZE))
    {
        num_current_allocs += 1;
        self.allocate_bucket(num_current_allocs as usize);
    }
}

```

And that's it for memory. We can now do every thing we need to do to access and
top up the vector's memory. Now's time for the really hard part: actually
implementing the vector's functions.

---

### Complete source for `reserve()`

```rust
pub fn reserve(&self, size: usize) {
    // Cache the size to prevent another atomic op from due to calling `size()` again
    let current_size = self.size();
    if current_size == 0 {
        self.allocate_bucket(0);
    }

    // Number of allocations needed for current size
    let mut num_current_allocs =
        highest_bit(current_size.saturating_add(FIRST_BUCKET_SIZE) - 1)
            .saturating_sub(highest_bit(FIRST_BUCKET_SIZE));

    // Compare with the number of allocations needed for size `new`
    while num_current_allocs
        < highest_bit(size.saturating_add(FIRST_BUCKET_SIZE) - 1)
            .saturating_sub(highest_bit(FIRST_BUCKET_SIZE))
    {
        num_current_allocs += 1;
        self.allocate_bucket(num_current_allocs as usize);
    }
}

```
