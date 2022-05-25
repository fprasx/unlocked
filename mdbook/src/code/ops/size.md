# `size()`

The procedure for `size` is simple. Load the vector's size from the
`Descriptor`, then subtract one if there is a pending write.

It seems like so long ago that we went over
[The Algorithm](../../paper/algorithm.md), but recall that when we perform a
`push`, we swap in a `Descriptor`, then call `complete_write`. This means that
when we do a write, the increase in size is reflected in the vector's state
before the write actually happens. If there is still a `WriteDescriptor`
contained in the `Descriptor`, that means the size stored in the `Descriptor` is
one greater than the actual size of the vector, because `complete_write`
replaces the `WriteDescriptor` with `None` when it executes the write.

Here is the code:

```rust
pub fn size(&self) -> usize {
    // # Safety
    // The pointers are safe to dereference because they all came from `Box::into_raw`
    // and point to valid objects

    let desc = unsafe { &*self.descriptor.load(Ordering::Acquire) };
    let size = desc.size;

    // If there is a pending descriptor, we subtract one from the size because
    // `push` increments the size, swaps the new descriptor in, and _then_ writes
    // the value. Therefore the size is one greater because the write hasn't happened
    // yet
    match unsafe { &*desc.pending.load(Ordering::Acquire) } {
        Some(_) => size - 1,
        None => size,
    }
}
```

There have been many momentous moments throughout the book: understanding the
algorithm, finishing `push`, and finally, completing the vector's public API.
When I was writing the code, this moment felt huge, and I jumped up and down
after `push`ing 10 elements onto the vector, `pop`ing 10 times, and running
`assert_eq!(sv.size(), 0);` withough crashing.

Let's run some tests (more fun than you might think)!
