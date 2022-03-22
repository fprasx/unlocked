# Memory Allocation

Memory allocation is probably the easiest part of implementing the vector. It's
almost the same as allocating memory for a normal vector, except the one small
part which involves actually mutating the vector. This part requires some
synchronization through compare-and-swap.

Let's dive into the Rust code.

# get()

The first, and simplest function to write is vector.get(i), which returns a
pointer to the element at index _i_. 

Here's the code in Rust:
```rust

```
