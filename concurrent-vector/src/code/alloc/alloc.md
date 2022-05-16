# Memory Allocation

The first thing I did when writing the code out was think about the pieces that
would make up the vector. In Rust, an extremely common building block for any
type is the `struct`. A `struct` just sticks its members' data next to each
other in memory. Here is the vector itself, as a struct:

```rust
pub struct SecVec<'a, T: Sized + Copy + Send + Sync> {
    buffers: CachePadded<Box<[AtomicPtr<AtomicU64>; 60]>>,
    descriptor: CachePadded<AtomicPtr<Descriptor<'a, T>>>,
    _marker: PhantomData<T>, // Data is stored as transmuted T's
}
```

There is a lot to unpack here. Firstly, `CachePadded<T>` is a `struct` provided
by the crate `crossbeam_utils` crate.

> A note on cache: a cache is a small section of memory implemented on the
> hardware level that allows for extremely fast access. Because of the way it's
> build on a hardware level, large amounts of cache are unfeasible. Cache is divided into segments called _cache lines_. When the CPU loads a value from cache, it loads in the whole cache line to find the value it wants.

# get()

The first, and simplest function to write is vector.get(i), which returns a
pointer to the element at index _i_.

Here's the code in Rust:

```rust

```
